//! The Claude Code or Codex session that started a server.
//!
//! Both agents export their session id to every command they run
//! (`CLAUDE_CODE_SESSION_ID`, `CODEX_THREAD_ID`), so a server's environment
//! names its session exactly.

use super::{env, home, Cache, Stamped};
use crate::platform::ProcDetails;
use crate::protocol::{AgentKind, AgentSession};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

pub const CLAUDE_SESSION_ENV: &str = "CLAUDE_CODE_SESSION_ID";
pub const CODEX_THREAD_ENV: &str = "CODEX_THREAD_ID";

/// Titles change as a session goes, so they are re-read this often.
const TITLE_LIFETIME: Duration = Duration::from_secs(30);
/// How long before a Codex transcript not found yet is looked for again.
const RETRY: Duration = Duration::from_secs(60);

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
            claude: dir("CLAUDE_CONFIG_DIR", ".claude"),
            codex: dir("CODEX_HOME", ".codex"),
        }
    }
}

pub struct Resolver {
    dirs: Dirs,
    claude: Cache<String, AgentSession>,
    codex: Cache<String, Option<CodexSession>>,
    codex_titles: Cache<(), Arc<HashMap<String, String>>>,
}

/// The first line of a Codex transcript.
#[derive(Clone)]
struct CodexSession {
    id: String,
    cwd: String,
    transcript: PathBuf,
}

impl Resolver {
    pub fn new(dirs: Dirs) -> Self {
        Self {
            dirs,
            claude: Cache::default(),
            codex: Cache::default(),
            codex_titles: Cache::default(),
        }
    }

    pub fn agent(&self, chain: &[ProcDetails]) -> Option<AgentSession> {
        if let Some(id) = env(chain, CLAUDE_SESSION_ENV).filter(|id| is_id(id)) {
            return Some(self.claude(id));
        }
        if let Some(id) = env(chain, CODEX_THREAD_ENV).filter(|id| is_id(id)) {
            let found = self.codex.get(&id.to_string(), || {
                let found = self.find_codex(id).and_then(|path| codex_header(&path));
                // A transcript, once found, stays put. Look again later for one not written yet.
                let stamped = Stamped::new(vec![], found.clone());
                if found.is_some() {
                    stamped
                } else {
                    stamped.expires_in(RETRY)
                }
            });
            return Some(match found {
                Some(session) => self.codex_session(&session),
                None => session(AgentKind::Codex, id, None),
            });
        }
        None
    }

    // ------------------------------------------------------------ Claude Code

    fn claude(&self, id: &str) -> AgentSession {
        self.claude.get(&id.to_string(), || {
            let mut found = session(AgentKind::ClaudeCode, id, None);
            if let Some(transcript) = self.find_claude(id) {
                // Titles are appended as the session goes, so the newest is near the end.
                let head = read_head(&transcript, 256 * 1024);
                found.title = last_title(&read_tail(&transcript, 512 * 1024))
                    .or_else(|| last_title(&head))
                    .or_else(|| first_prompt(&head));
                found.directory =
                    json_lines(&head).find_map(|l| Some(l["cwd"].as_str()?.to_string()));
                found.started_at = created_ms(&transcript);
                found.transcript_path = Some(transcript.to_string_lossy().into_owned());
            }
            Stamped::new(vec![], found).expires_in(TITLE_LIFETIME)
        })
    }

    /// `<claude>/projects/<folder>/<id>.jsonl`.
    fn find_claude(&self, id: &str) -> Option<PathBuf> {
        let projects = self.dirs.claude.as_ref()?.join("projects");
        let file = format!("{id}.jsonl");
        fs::read_dir(projects)
            .ok()?
            .flatten()
            .map(|e| e.path().join(&file))
            .find(|p| p.is_file())
    }

    // ------------------------------------------------------------ Codex

    fn codex_session(&self, found: &CodexSession) -> AgentSession {
        let mut agent = session(AgentKind::Codex, &found.id, Some(found.cwd.clone()));
        agent.title = self.codex_titles().get(&found.id).cloned();
        agent.started_at = created_ms(&found.transcript);
        agent.transcript_path = Some(found.transcript.to_string_lossy().into_owned());
        agent
    }

    /// Thread names from `session_index.jsonl`, re-read when it changes.
    fn codex_titles(&self) -> Arc<HashMap<String, String>> {
        let Some(index) = self
            .dirs
            .codex
            .as_ref()
            .map(|c| c.join("session_index.jsonl"))
        else {
            return Arc::default();
        };
        self.codex_titles.get(&(), || {
            let text = read_tail(&index, 1024 * 1024);
            let titles = json_lines(&text)
                .filter_map(|l| {
                    Some((
                        l["id"].as_str()?.to_string(),
                        l["thread_name"].as_str()?.to_string(),
                    ))
                })
                .collect();
            Stamped::new(vec![index.clone()], Arc::new(titles))
        })
    }

    /// `<codex>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`, newest days first.
    fn find_codex(&self, id: &str) -> Option<PathBuf> {
        let suffix = format!("-{id}.jsonl");
        self.codex_days().into_iter().find_map(|day| {
            files(&day)
                .into_iter()
                .find(|f| f.to_string_lossy().ends_with(&suffix))
        })
    }

    fn codex_days(&self) -> Vec<PathBuf> {
        let Some(sessions) = self.dirs.codex.as_ref().map(|c| c.join("sessions")) else {
            return Vec::new();
        };
        let newest_first = |dir: &Path| {
            let mut dirs: Vec<PathBuf> = files(dir).into_iter().filter(|p| p.is_dir()).collect();
            dirs.sort_by(|a, b| b.cmp(a));
            dirs
        };
        newest_first(&sessions)
            .iter()
            .flat_map(|year| newest_first(year))
            .flat_map(|month| newest_first(&month))
            .collect()
    }
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

fn codex_header(transcript: &Path) -> Option<CodexSession> {
    let mut first = String::new();
    BufReader::new(File::open(transcript).ok()?.take(1024 * 1024))
        .read_line(&mut first)
        .ok()?;
    let line: serde_json::Value = serde_json::from_str(&first).ok()?;
    let payload = &line["payload"];
    Some(CodexSession {
        id: payload["id"].as_str()?.to_string(),
        cwd: payload["cwd"].as_str()?.to_string(),
        transcript: transcript.to_path_buf(),
    })
}

/// A custom title (`/rename`) wins over the generated one. The last of each counts.
fn last_title(text: &str) -> Option<String> {
    let (mut custom, mut generated) = (None, None);
    for line in json_lines(text) {
        match line["type"].as_str() {
            Some("custom-title") => custom = line["customTitle"].as_str().map(str::to_string),
            Some("ai-title") => generated = line["aiTitle"].as_str().map(str::to_string),
            _ => {}
        }
    }
    custom.or(generated).filter(|t| !t.is_empty())
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

fn files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).map_or_else(|_| Vec::new(), |d| d.flatten().map(|e| e.path()).collect())
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

/// The last `bytes` of a file, starting at a whole line.
fn read_tail(path: &Path, bytes: u64) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let offset = file.metadata().map_or(0, |m| m.len().saturating_sub(bytes));
    let mut data = Vec::new();
    if file.seek(SeekFrom::Start(offset)).is_ok() {
        let _ = file.read_to_end(&mut data);
    }
    let text = String::from_utf8_lossy(&data);
    match (offset > 0, text.find('\n')) {
        (true, Some(newline)) => text[newline + 1..].to_string(),
        _ => text.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{s, TempDir};
    use super::*;

    const CLAUDE_ID: &str = "68c8fda6-2f4e-4c1a-9a7b-1d2e3f4a5b6c";
    const CODEX_ID: &str = "01a0e8f9-fa4b-7371-b125-f55864458aa9";

    fn resolver(tmp: &TempDir) -> Resolver {
        Resolver::new(Dirs {
            claude: Some(tmp.path(".claude")),
            codex: Some(tmp.path(".codex")),
        })
    }

    fn process(cwd: Option<&str>, env: &[(&str, &str)]) -> ProcDetails {
        ProcDetails {
            cwd: cwd.map(str::to_string),
            args: Vec::new(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
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
        let workspace = "/Users/dev/conductor/workspaces/port-process-manager/providence";
        let transcript = tmp.write(
            &format!(".claude/projects/-Users-dev-conductor-workspaces-port-process-manager-providence/{CLAUDE_ID}.jsonl"),
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
            process(Some(workspace), &[]),
            process(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)]),
        ];

        let found = resolver(&tmp).agent(&chain).unwrap();
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
        let found = resolver(&tmp)
            .agent(&[process(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)])])
            .unwrap();
        assert_eq!(
            found.title.as_deref(),
            Some("Port the resolver from WhatThePort and keep every path worki")
        );
    }

    #[test]
    fn claude_session_without_a_transcript_can_still_be_resumed() {
        let tmp = TempDir::new();
        let found = resolver(&tmp)
            .agent(&[process(None, &[(CLAUDE_SESSION_ENV, CLAUDE_ID)])])
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
        let chain = [process(
            Some("/Users/dev/app"),
            &[(CODEX_THREAD_ENV, CODEX_ID)],
        )];

        let found = resolver(&tmp).agent(&chain).unwrap();
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
        let chain = [process(
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
    fn unsafe_ids_are_ignored() {
        let tmp = TempDir::new();
        let chain = [process(None, &[(CLAUDE_SESSION_ENV, "../../secrets")])];
        assert_eq!(resolver(&tmp).agent(&chain), None);
    }
}
