//! What a server belongs to: its project, workspace and coding-agent session.
//! Owned by the links lane.
//!
//! The engine calls these on every scan, so each result is cached and reused
//! until one of the files it was read from changes. A steady-state call costs
//! a few `stat`s.

mod agent;
mod preview;
mod project;

use crate::platform::ProcDetails;
use crate::protocol::{AgentSession, Project, Workspace};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

pub use preview::set_vercel_previews;

/// Environment variables `details()` should read for agent and workspace detection.
pub const AGENT_ENV_KEYS: &[&str] = &[
    agent::CLAUDE_SESSION_ENV,
    agent::CODEX_THREAD_ENV,
    project::PANE_SESSION_ENV,
    project::PANE_PANEL_ENV,
    project::PANE_WORKSPACE_ENV,
    project::CONDUCTOR_WORKSPACE_ENV,
];

static PROJECTS: LazyLock<project::Resolver> = LazyLock::new(project::Resolver::default);
static AGENTS: LazyLock<agent::Resolver> =
    LazyLock::new(|| agent::Resolver::new(agent::Dirs::from_env()));

/// `command` is the server's command line. It tells a Storybook server apart
/// from the app it lives in.
pub fn project(cwd: &str, command: Option<&str>) -> Project {
    PROJECTS.project(Path::new(cwd), command)
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

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A value read from files, fresh while none of them has changed and, when
/// it has a lifetime, until that runs out.
struct Stamped<T> {
    files: Vec<(PathBuf, Option<SystemTime>)>,
    until: Option<Instant>,
    value: T,
}

impl<T: Clone> Stamped<T> {
    fn new(files: Vec<PathBuf>, value: T) -> Self {
        let files = files
            .into_iter()
            .map(|f| {
                let at = modified(&f);
                (f, at)
            })
            .collect();
        Self {
            files,
            until: None,
            value,
        }
    }

    fn expires_in(self, lifetime: Duration) -> Self {
        Self {
            until: Some(Instant::now() + lifetime),
            ..self
        }
    }

    fn is_fresh(&self) -> bool {
        self.until.is_none_or(|until| Instant::now() < until)
            && self.files.iter().all(|(path, at)| modified(path) == *at)
    }
}

/// A cache of `Stamped` values. `get` returns the cached value while its
/// files are unchanged, and calls `read` otherwise.
struct Cache<K, T>(Mutex<std::collections::HashMap<K, Stamped<T>>>);

impl<K, T> Default for Cache<K, T> {
    fn default() -> Self {
        Self(Mutex::default())
    }
}

impl<K: std::hash::Hash + Eq + Clone, T: Clone> Cache<K, T> {
    fn get(&self, key: &K, read: impl FnOnce() -> Stamped<T>) -> T {
        if let Some(hit) = self.0.lock().unwrap().get(key).filter(|s| s.is_fresh()) {
            return hit.value.clone();
        }
        let fresh = read();
        let value = fresh.value.clone();
        self.0.lock().unwrap().insert(key.clone(), fresh);
        value
    }
}

#[cfg(test)]
mod fixture {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh folder under the system temp dir, removed on drop.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ppm-links-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // Canonical, so paths match what the OS reports (/private/var on macOS).
            Self(dir.canonicalize().unwrap())
        }

        pub fn write(&self, rel: &str, contents: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            path
        }

        pub fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
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
}
