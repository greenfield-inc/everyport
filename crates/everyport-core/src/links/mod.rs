//! What a server belongs to: its project, workspace and coding-agent session.
//! Owned by the links lane.
//!
//! The engine calls these on every scan. Each result is cached until a file
//! it was read from changes (see `watch`), so a steady-state call touches no
//! files. Results no scan asked for in a minute are dropped.

mod agent;
mod preview;
mod project;
mod watch;

use crate::platform::ProcDetails;
use crate::protocol::{AgentSession, Project, Workspace};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub use preview::set_vercel_previews;

/// Environment variables `details()` should read for agent and workspace detection.
pub const AGENT_ENV_KEYS: &[&str] = &[
    agent::CLAUDE_SESSION_ENV,
    agent::CODEX_THREAD_ENV,
    project::PANE_SESSION_ENV,
    project::PANE_PANEL_ENV,
    project::PANE_WORKSPACE_ENV,
    project::CONDUCTOR_WORKSPACE_ENV,
    agent::CLAUDE_DIR_ENV,
    agent::CODEX_DIR_ENV,
];

static PROJECTS: LazyLock<project::Resolver> = LazyLock::new(project::Resolver::default);
static AGENTS: LazyLock<agent::Resolver> =
    LazyLock::new(|| agent::Resolver::new(agent::Dirs::from_env()));

/// `command` is the server's command line. It tells a Storybook server apart
/// from the app it lives in. `process_name` names a project whose folder
/// can't, such as `/` or Homebrew's `var/postgresql@15`.
pub fn project(cwd: &str, command: Option<&str>, process_name: &str) -> Project {
    PROJECTS.project(Path::new(cwd), command, process_name)
}

/// `chain` is the server's process and its ancestors, nearest first. The
/// environment (Pane, Conductor) beats the folder layout.
pub fn workspace(cwd: &str, chain: &[ProcDetails]) -> Option<Workspace> {
    PROJECTS.workspace(Path::new(cwd), chain)
}

/// `chain` is the server's process and its ancestors, nearest first.
pub fn agent(chain: &[ProcDetails]) -> Option<AgentSession> {
    AGENTS.agent(chain)
}

/// The nearest value of `key` in `chain`.
fn env<'a>(chain: &'a [ProcDetails], key: &str) -> Option<&'a str> {
    chain.iter().find_map(|p| {
        p.env
            .iter()
            .find(|(k, v)| k == key && !v.is_empty())
            .map(|(_, v)| v.as_str())
    })
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod fixture {
    use crate::platform::ProcDetails;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh folder under the system temp dir, removed on drop.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("everyport-links-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub fn write(&self, rel: &str, contents: &str) -> PathBuf {
            let path = self.path(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            path
        }

        /// `rel` uses `/`; the result uses the OS separator, as paths read
        /// back from the filesystem do.
        pub fn path(&self, rel: &str) -> PathBuf {
            rel.split('/')
                .fold(self.0.clone(), |path, part| path.join(part))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn s(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// Polls `check` until it returns a value. File events arrive after a
    /// 1.5 s batch, and discovery runs on a background thread.
    pub fn eventually<T>(check: impl Fn() -> Option<T>) -> T {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Some(value) = check() {
                return value;
            }
            assert!(std::time::Instant::now() < deadline, "timed out");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// One process of a server's chain.
    pub fn details(cwd: Option<&str>, env: &[(&str, &str)]) -> ProcDetails {
        ProcDetails {
            cwd: cwd.map(str::to_string),
            args: Vec::new(),
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}
