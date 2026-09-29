//! What a server belongs to: its project, workspace and coding-agent session.
//! Pure functions over the filesystem and process facts. Owned by the links lane.

use crate::platform::ProcDetails;
use crate::protocol::{AgentSession, Project, Workspace};

/// Environment variables `details()` should read for agent detection.
pub const AGENT_ENV_KEYS: &[&str] = &[];

pub fn project(cwd: &str) -> Project {
    let name = cwd
        .rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(cwd);
    Project {
        name: name.to_string(),
        root: None,
        framework: None,
        branch: None,
        worktree: None,
        github: None,
        vercel: None,
    }
}

pub fn workspace(_cwd: &str) -> Option<Workspace> {
    None
}

/// `chain` is the server's process and its ancestors, nearest first.
pub fn agent(_chain: &[ProcDetails]) -> Option<AgentSession> {
    None
}
