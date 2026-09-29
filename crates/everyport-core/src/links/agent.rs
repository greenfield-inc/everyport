//! The Claude Code or Codex session that started a server.
//!
//! Both agents export their session id to every command they run
//! (`CLAUDE_CODE_SESSION_ID`, `CODEX_THREAD_ID`), so a server's environment
//! names its session exactly. Its transcript is found once, on a background
//! thread. After that, only the lines appended to the watched transcript or
//! title index are read, when it changes.

use super::watch::Cache;
use super::{env, home};
use crate::platform::ProcDetails;
use crate::protocol::{AgentKind, AgentSession};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::UNIX_EPOCH;

pub const CLAUDE_SESSION_ENV: &str = "CLAUDE_CODE_SESSION_ID";
pub const CODEX_THREAD_ENV: &str = "CODEX_THREAD_ID";
/// Where each agent keeps its sessions, when not in the home folder. Read
/// from the server's environment, since an app started from the Dock or Start
/// menu doesn't have the user's shell variables.
pub const CLAUDE_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
pub const CODEX_DIR_ENV: &str = "CODEX_HOME";

/// How much of a transcript or title index to read the first time.
const FIRST_READ: u64 = 512 * 1024;

/// Where sessions live when the server's environment doesn't say.
pub struct Dirs {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

impl Dirs {
    /// `CLAUDE_CONFIG_DIR` and `CODEX_HOME` when set, else `~/.claude` and `~/.codex`.
    pub fn from_env() -> Self {
        let home = home();
        let dir = |var: &str, default: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(default)))
        };
        Self {
            claude: dir(CLAUDE_DIR_ENV, ".claude"),
            codex: dir(CODEX_DIR_ENV, ".codex"),
        }
    }
}

pub struct Resolver {
    dirs: Dirs,
    claude: Cache<String, Found<ClaudeTranscript>>,
    codex: Cache<String, Found<CodexTranscript>>,
    claude_titles: Cache<PathBuf, Appended<ClaudeTitles>>,
    codex_titles: Cache<PathBuf, Appended<Arc<HashMap<String, String>>>>,
}

/// Filled in by a background thread; empty until it finishes.
type Found<T> = Arc<OnceLock<Option<T>>>;

/// What the start of a Claude Code transcript says.
struct ClaudeTranscript {
    path: PathBuf,
    directory: Option<String>,
    started_at: Option<u64>,
    /// A title set early on, or else the first prompt.
    early_title: Option<String>,
}

/// The first line of a Codex transcript, and where its title lives.
struct CodexTranscript {
    path: PathBuf,
    cwd: String,
    started_at: Option<u64>,
    title_index: PathBuf,
}

impl Resolver {
    pub fn new(dirs: Dirs) -> Self {
        Self {
            dirs,
            claude: Cache::default(),
            codex: Cache::default(),
            claude_titles: Cache::default(),
            codex_titles: Cache::default(),
        }
    }

    pub fn agent(&self, chain: &[ProcDetails]) -> Option<AgentSession> {
        let dir = |key, default: &Option<PathBuf>| {
            env(chain, key)
                .map(PathBuf::from)
                .or_else(|| default.clone())
        };
        if let Some(id) = env(chain, CLAUDE_SESSION_ENV).filter(|id| is_id(id)) {
            return Some(self.claude(id, dir(CLAUDE_DIR_ENV, &self.dirs.claude)));
        }
        if let Some(id) = env(chain, CODEX_THREAD_ENV).filter(|id| is_id(id)) {
            return Some(self.codex(id, dir(CODEX_DIR_ENV, &self.dirs.codex)));
        }
        None
    }

    fn claude(&self, id: &str, dir: Option<PathBuf>) -> AgentSession {
        let mut agent = session(AgentKind::ClaudeCode, id, None);
        let found = self.claude.get(&id.to_string(), |_| {
            let id = id.to_string();
            discover(move || ClaudeTranscript::read(find_claude(&dir?, &id)?))
        });
        let Some(Some(transcript)) = found.get() else {
            return agent;
        };
        let path = &transcript.path;
        let titles = self.claude_titles.get(path, |previous| {
            let read = read_appended(path, previous, ClaudeTitles::default(), ClaudeTitles::add);
            (vec![path.clone()], read)
        });
        agent.title = titles
            .state
            .title()
            .or_else(|| transcript.early_title.clone());
        agent.directory = transcript.directory.clone();
        agent.started_at = transcript.started_at;
        agent.transcript_path = Some(path.to_string_lossy().into_owned());
        agent
    }

    fn codex(&self, id: &str, dir: Option<PathBuf>) -> AgentSession {
        let found = self.codex.get(&id.to_string(), |_| {
            let id = id.to_string();
            discover(move || {
                let dir = dir?;
                CodexTranscript::read(find_codex(&dir, &id)?, dir.join("session_index.jsonl"))
            })
        });
        let Some(Some(transcript)) = found.get() else {
            return session(AgentKind::Codex, id, None);
        };
        let index = &transcript.title_index;
        let titles = self.codex_titles.get(index, |previous| {
            let read = read_appended(index, previous, Arc::default(), |titles, line| {
                if let (Some(id), Some(name)) = (line["id"].as_str(), line["thread_name"].as_str())
                {
                    Arc::make_mut(titles).insert(id.to_string(), name.to_string());
                }
            });
            (vec![index.clone()], read)
        });
        let mut agent = session(AgentKind::Codex, id, Some(transcript.cwd.clone()));
        agent.title = titles.state.get(id).cloned();
        agent.started_at = transcript.started_at;
        agent.transcript_path = Some(transcript.path.to_string_lossy().into_owned());
        agent
    }
}

/// Runs `find` on a background thread, so discovery never holds up a scan.
fn discover<T: Send + Sync + 'static>(
    find: impl FnOnce() -> Option<T> + Send + 'static,
) -> (Vec<PathBuf>, Found<T>) {
    let found = Found::default();
    let slot = found.clone();
    std::thread::spawn(move || slot.set(find()));
    (Vec::new(), found)
}

impl ClaudeTranscript {
    fn read(path: PathBuf) -> Option<Self> {
        let head = read_head(&path, 256 * 1024);
        let early_title = json_lines(&head)
            .fold(ClaudeTitles::default(), |mut titles, line| {
                titles.add(&line);
                titles
            })
            .title()
            .or_else(|| first_prompt(&head));
        let directory = json_lines(&head).find_map(|l| Some(l["cwd"].as_str()?.to_string()));
        Some(Self {
            directory,
            started_at: created_ms(&path),
            early_title,
            path,
        })
    }
}

impl CodexTranscript {
    fn read(path: PathBuf, title_index: PathBuf) -> Option<Self> {
        let mut first = String::new();
        BufReader::new(File::open(&path).ok()?.take(1024 * 1024))
            .read_line(&mut first)
            .ok()?;
        let line: serde_json::Value = serde_json::from_str(&first).ok()?;
        Some(Self {
            cwd: line["payload"]["cwd"].as_str()?.to_string(),
            started_at: created_ms(&path),
            path,
            title_index,
        })
    }
}

/// A custom title (`/rename`) wins over the generated one. The last of each counts.
#[derive(Clone, Default)]
struct ClaudeTitles {
    custom: Option<String>,
    generated: Option<String>,
}

impl ClaudeTitles {
    fn add(&mut self, line: &serde_json::Value) {
        let text = |key: &str| {
            line[key]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        match line["type"].as_str() {
            Some("custom-title") => self.custom = text("customTitle").or(self.custom.take()),
            Some("ai-title") => self.generated = text("aiTitle").or(self.generated.take()),
            _ => {}
        }
    }

    fn title(&self) -> Option<String> {
        self.custom.clone().or_else(|| self.generated.clone())
    }
}

/// State folded from a JSON-lines file, and how far into it that got.
#[derive(Clone)]
struct Appended<S> {
    offset: u64,
    state: S,
}

/// Folds the whole lines added to `path` since `previous` was read. The first
/// time, or if the file shrank, starts from its last `FIRST_READ` bytes.
fn read_appended<S>(
    path: &Path,
    previous: Option<Appended<S>>,
    empty: S,
    add: impl Fn(&mut S, &serde_json::Value),
) -> Appended<S> {
    let len = fs::metadata(path).map_or(0, |m| m.len());
    let (start, mut state, resume) = match previous {
        Some(p) if p.offset <= len => (p.offset, p.state, true),
        _ => (len.saturating_sub(FIRST_READ), empty, false),
    };
    let mut data = Vec::new();
    if let Ok(mut file) = File::open(path) {
        if file.seek(SeekFrom::Start(start)).is_ok() {
            let _ = file.take(len - start).read_to_end(&mut data);
        }
    }
    // Stop at the last complete line; a line still being written is read next time.
    let end = data.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut text = String::from_utf8_lossy(&data[..end]);
    if start > 0 && !resume {
        // Started mid-file: drop the partial first line.
        let skip = text.find('\n').map_or(text.len(), |i| i + 1);
        text = text[skip..].to_string().into();
    }
    for line in json_lines(&text) {
        add(&mut state, &line);
    }
    Appended {
        offset: start + end as u64,
        state,
    }
}

/// `<claude>/projects/<folder>/<id>.jsonl`.
fn find_claude(claude: &Path, id: &str) -> Option<PathBuf> {
    let file = format!("{id}.jsonl");
    fs::read_dir(claude.join("projects"))
        .ok()?
        .flatten()
        .map(|e| e.path().join(&file))
        .find(|p| p.is_file())
}

/// `<codex>/sessions/YYYY/MM/DD/rollout-<local time>-<id>.jsonl`. Codex ids are
/// UUIDv7, which start with their creation time in Unix milliseconds, so only
/// the folders for that UTC day and the days either side (time zones) are read.
fn find_codex(codex: &Path, id: &str) -> Option<PathBuf> {
    let ms = u64::from_str_radix(id.replace('-', "").get(..12)?, 16).ok()?;
    let day = (ms / 86_400_000) as i64;
    let suffix = format!("-{id}.jsonl");
    [day, day - 1, day + 1].into_iter().find_map(|day| {
        let (y, m, d) = civil_from_days(day);
        let folder = codex
            .join("sessions")
            .join(format!("{y:04}"))
            .join(format!("{m:02}"))
            .join(format!("{d:02}"));
        fs::read_dir(folder)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|f| f.to_string_lossy().ends_with(&suffix))
    })
}

/// Year, month and day of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn session(kind: AgentKind, id: &str, directory: Option<String>) -> AgentSession {
    let resume = match kind {
        AgentKind::ClaudeCode => "claude --resume",
        AgentKind::Codex => "codex resume",
    };
    AgentSession {
        kind,
        id: id.to_string(),
        title: None,
        started_at: None,
        transcript_path: None,
        directory,
        resume_command: format!("{resume} {id}"),
    }
}

/// Session ids are UUIDs; anything else could point outside the transcript folders.
fn is_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The first line of the first typed prompt, up to 60 characters.
fn first_prompt(text: &str) -> Option<String> {
    json_lines(text)
        .filter(|l| l["type"] == "user")
        .filter_map(|l| Some(l["message"]["content"].as_str()?.trim().to_string()))
        .find(|p| !p.is_empty() && !p.starts_with('<'))
        .map(|p| {
            p.lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(60)
                .collect()
        })
}

fn json_lines(text: &str) -> impl Iterator<Item = serde_json::Value> + '_ {
    text.lines().filter_map(|l| serde_json::from_str(l).ok())
}

fn created_ms(path: &Path) -> Option<u64> {
    let created = fs::metadata(path).and_then(|m| m.created()).ok()?;
    Some(created.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

fn read_head(path: &Path, bytes: u64) -> String {
    let mut data = Vec::new();
    if let Ok(file) = File::open(path) {
        let _ = file.take(bytes).read_to_end(&mut data);
    }
    String::from_utf8_lossy(&data).into_owned()
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{details, eventually, s, TempDir};
    use super::*;

    const CLAUDE_ID: &str = "68c8fda6-2f4e-4c1a-9a7b-1d2e3f4a5b6c";
    const CODEX_ID: &str = "01a0e8f9-fa4b-7371-b125-f55864458aa9";

    fn resolver(tmp: &TempDir) -> Resolver {
        Resolver::new(Dirs {
            claude: Some(tmp.path(".claude")),
            codex: Some(tmp.path(".codex")),
        })
    }

    /// Discovery runs in the background, so ask until it has finished.
    fn discovered(resolver: &Resolver, chain: &[ProcDetails]) -> AgentSession {
        eventually(|| {
            resolver
                .agent(chain)
                .filter(|a| a.transcript_path.is_some())
        })
    }

    /// A Codex transcript's first line, as Codex 0.157 writes it.
    fn codex_transcript(tmp: &TempDir, day: &str, id: &str, cwd: &str) -> PathBuf {
        tmp.write(
            &format!(".codex/sessions/{day}/rollout-2026-09-28T10-04-51-{id}.jsonl"),
            &format!(
                "{}\n{{\"type\":\"event_msg\"}}\n",
                serde_json::json!({"timestamp": "2026-09-28T17:04:51.566Z", "type": "session_meta",
                    "payload": {"id": id, "timestamp": "2026-09-28T17:04:51.337Z", "cwd": cwd, "originator": "codex-tui"}})
            ),
        )
    }

    #[test]
    fn claude_session_from_its_environment() {
        let tmp = TempDir::new();
        let workspace = "/Users/dev/conductor/workspaces/everyport/providence";
        let transcript = tmp.write(
            &format!(".claude/projects/-Users-dev-conductor-workspaces-everyport-providence/{CLAUDE_ID}.jsonl"),
            &[
                r#"{"type":"permission-mode","permissionMode":"default","sessionId":"x"}"#.to_string(),
                serde_json::json!({"type": "user", "cwd": workspace, "message": {"role": "user", "content": "Make the menu bar icon a dot grid\nwith 3 rows"}}).to_string(),
                r#"{"type":"ai-title","aiTitle":"Menu bar icon","sessionId":"x"}"#.to_string(),
                r#"{"type":"custom-title","customTitle":"Dot-grid menu bar icon","sessionId":"x"}"#.to_string(),
                r#"{"type":"ai-title","aiTitle":"Dot grid icon polish","sessionId":"x"}"#.to_string(),
            ]
            .join("\n"),
        );
        // The session id survives on `npm`, not on the renamed `next-server` below it.
        let chain = [
            details(Some(workspace), &[]),
            details(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)]),
        ];

        let found = discovered(&resolver(&tmp), &chain);
        assert_eq!(found.kind, AgentKind::ClaudeCode);
        assert_eq!(found.title.as_deref(), Some("Dot-grid menu bar icon"));
        assert_eq!(found.directory.as_deref(), Some(workspace));
        assert_eq!(found.transcript_path, Some(s(&transcript)));
        assert_eq!(found.resume_command, format!("claude --resume {CLAUDE_ID}"));
        assert!(found.started_at.is_some());
    }

    #[test]
    fn untitled_claude_session_uses_its_first_prompt() {
        let tmp = TempDir::new();
        tmp.write(
            &format!(".claude/projects/-tmp-app/{CLAUDE_ID}.jsonl"),
            &[
                serde_json::json!({"type": "user", "message": {"content": "<command-name>/clear</command-name>"}}).to_string(),
                serde_json::json!({"type": "user", "message": {"content": "  Port the resolver from WhatThePort and keep every path working on Windows, Linux and macOS  "}}).to_string(),
            ]
            .join("\n"),
        );
        let chain = [details(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)])];
        let found = discovered(&resolver(&tmp), &chain);
        assert_eq!(
            found.title.as_deref(),
            Some("Port the resolver from WhatThePort and keep every path worki")
        );
    }

    #[test]
    fn claude_session_without_a_transcript_can_still_be_resumed() {
        let tmp = TempDir::new();
        let found = resolver(&tmp)
            .agent(&[details(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)])])
            .unwrap();
        assert_eq!((found.title, found.transcript_path), (None, None));
        assert_eq!(found.resume_command, format!("claude --resume {CLAUDE_ID}"));
    }

    #[test]
    fn codex_session_from_its_environment() {
        let tmp = TempDir::new();
        let transcript = codex_transcript(&tmp, "2026/09/28", CODEX_ID, "/Users/dev/app");
        codex_transcript(
            &tmp,
            "2026/09/27",
            "01a0e4a4-b3cb-7c01-ae42-7bb500166386",
            "/Users/dev/app",
        );
        tmp.write(
            ".codex/session_index.jsonl",
            &format!(
                "{{\"id\":\"{CODEX_ID}\",\"thread_name\":\"Rebase session PRs\"}}\n{{\"id\":\"{CODEX_ID}\",\"thread_name\":\"Rebase and prepare session PRs\"}}\n"
            ),
        );
        let chain = [details(
            Some("/Users/dev/app"),
            &[(CODEX_THREAD_ENV, CODEX_ID)],
        )];

        let found = discovered(&resolver(&tmp), &chain);
        assert_eq!(found.kind, AgentKind::Codex);
        assert_eq!(found.id, CODEX_ID);
        assert_eq!(
            found.title.as_deref(),
            Some("Rebase and prepare session PRs")
        );
        assert_eq!(found.directory.as_deref(), Some("/Users/dev/app"));
        assert_eq!(found.transcript_path, Some(s(&transcript)));
        assert_eq!(found.resume_command, format!("codex resume {CODEX_ID}"));
    }

    #[test]
    fn claude_wins_when_both_agents_are_in_the_environment() {
        let tmp = TempDir::new();
        let chain = [details(
            None,
            &[
                (CODEX_THREAD_ENV, CODEX_ID),
                (CLAUDE_SESSION_ENV, CLAUDE_ID),
            ],
        )];
        assert_eq!(
            resolver(&tmp).agent(&chain).unwrap().kind,
            AgentKind::ClaudeCode
        );
    }

    #[test]
    fn sessions_folder_from_the_servers_environment() {
        let tmp = TempDir::new();
        let transcript = tmp.write(
            &format!("work-claude/projects/-tmp-app/{CLAUDE_ID}.jsonl"),
            r#"{"type":"ai-title","aiTitle":"Fix the login redirect"}"#,
        );
        // everyport itself, started from the Dock, doesn't have the shell's CLAUDE_CONFIG_DIR.
        let chain = [details(
            None,
            &[
                (CLAUDE_SESSION_ENV, CLAUDE_ID),
                (CLAUDE_DIR_ENV, &s(&tmp.path("work-claude"))),
            ],
        )];
        let found = discovered(&resolver(&tmp), &chain);
        assert_eq!(found.transcript_path, Some(s(&transcript)));
        assert_eq!(found.title.as_deref(), Some("Fix the login redirect"));
    }

    #[test]
    fn a_renamed_session_shows_its_new_title() {
        let tmp = TempDir::new();
        let transcript = tmp.write(
            &format!(".claude/projects/-tmp-app/{CLAUDE_ID}.jsonl"),
            "{\"type\":\"ai-title\",\"aiTitle\":\"Menu bar icon\"}\n",
        );
        let resolver = resolver(&tmp);
        let chain = [details(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)])];
        assert_eq!(
            discovered(&resolver, &chain).title.as_deref(),
            Some("Menu bar icon")
        );

        // What `/rename` appends.
        let mut file = File::options().append(true).open(&transcript).unwrap();
        std::io::Write::write_all(
            &mut file,
            b"{\"type\":\"custom-title\",\"customTitle\":\"Dot-grid menu bar icon\"}\n",
        )
        .unwrap();
        let renamed = eventually(|| {
            resolver
                .agent(&chain)
                .filter(|a| a.title.as_deref() == Some("Dot-grid menu bar icon"))
        });
        assert_eq!(renamed.transcript_path, Some(s(&transcript)));
    }

    #[test]
    fn unsafe_ids_are_ignored() {
        let tmp = TempDir::new();
        let chain = [details(None, &[(CLAUDE_SESSION_ENV, "../../secrets")])];
        assert_eq!(resolver(&tmp).agent(&chain), None);
    }
}
