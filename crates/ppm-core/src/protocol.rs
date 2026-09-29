//! The ppm wire protocol. `ppm stdio` writes one `Event` per line to stdout and
//! reads one `Request` per line from stdin; `ppm serve` carries the same JSON
//! over HTTP and server-sent events. Types export to TypeScript through ts-rs
//! (`cargo test -p ppm-core` writes packages/protocol/src/generated).
//!
//! Timestamps are Unix milliseconds. Memory is bytes. CPU is percent of one core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Bump when a change breaks older clients. Additive fields do not bump it.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------- events (ppm -> client)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Event {
    /// First line on every connection.
    Hello(Hello),
    /// Full state, sent after `hello` and again whenever anything changes.
    Snapshot(Snapshot),
    /// Answer to the `Request` with the same id.
    Result(RequestResult),
    /// A server crossed a threshold. Clients decide how to notify.
    Alert(Alert),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Hello {
    pub protocol: u32,
    pub ppm_version: String,
    pub host: HostInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HostInfo {
    pub hostname: String,
    pub os: Os,
    /// Rust target arch, such as `aarch64` or `x86_64`.
    pub arch: String,
    pub cores: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Os {
    Macos,
    Linux,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Snapshot {
    #[ts(type = "number")]
    pub taken_at: u64,
    pub system: SystemStats,
    /// Sorted by port.
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemStats {
    #[ts(type = "number")]
    pub memory_total: u64,
    #[ts(type = "number")]
    pub memory_used: u64,
    /// Memory used by everything that is not a listed server.
    #[ts(type = "number")]
    pub memory_other_apps: u64,
    /// Whole-machine CPU, 0-100.
    pub cpu_percent: f32,
}

/// A listening port and the process tree behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Server {
    pub port: u16,
    /// Process that owns the socket.
    pub pid: u32,
    /// Topmost process of the server's tree, such as `npm run dev`.
    pub root: ProcRef,
    pub process_name: String,
    /// Bound addresses, such as `127.0.0.1` and `::1`.
    pub addresses: Vec<String>,
    pub cwd: Option<String>,
    /// False once the folder (for example a deleted worktree) is gone.
    pub cwd_exists: bool,
    /// Command line of the root process.
    pub command: Option<String>,
    /// Folder the root process was started from; `restart` runs `command` here.
    pub launch_dir: Option<String>,
    #[ts(type = "number | null")]
    pub started_at: Option<u64>,
    pub project: Project,
    pub workspace: Option<Workspace>,
    pub agent: Option<AgentSession>,
    /// Depth-first, root first.
    pub processes: Vec<ServerProcess>,
    /// Sum over `processes`.
    #[ts(type = "number")]
    pub memory: u64,
    pub cpu_percent: f32,
    pub connections: u32,
    /// Oldest first, covering up to the last 10 minutes.
    pub history: Vec<Sample>,
    #[ts(type = "number")]
    pub last_active: u64,
    /// Matches the protected list (databases and similar). Never stopped by clean up.
    pub protected: bool,
    pub status: ServerStatus,
    /// Present when Clean up suggests stopping this server.
    pub clean_up: Option<CleanUpReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcRef {
    pub pid: u32,
    /// Process start time. Actions check it so a reused pid is never signalled.
    #[ts(type = "number")]
    pub started_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServerProcess {
    pub proc: ProcRef,
    pub name: String,
    /// 0 for the root.
    pub depth: u32,
    #[ts(type = "number")]
    pub memory: u64,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sample {
    #[ts(type = "number")]
    pub at: u64,
    #[ts(type = "number")]
    pub memory: u64,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    /// package.json name, repo folder name, or cwd folder name.
    pub name: String,
    pub root: Option<String>,
    /// Such as `Next.js`, `Vite`, `Storybook`.
    pub framework: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    /// `owner/repo` of the GitHub remote, when there is one.
    pub github: Option<String>,
    pub vercel: Option<VercelProject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VercelProject {
    pub project_id: String,
    /// Preview URL for the current commit, when found.
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Workspace {
    pub kind: WorkspaceKind,
    pub name: String,
    /// Deep link that opens this workspace in its app, such as
    /// `pane://open?pane=<id>&panel=<id>` from `PANE_SESSION_ID`/`PANE_PANEL_ID`.
    pub open_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceKind {
    Conductor,
    Pane,
    GitWorktree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentSession {
    pub kind: AgentKind,
    pub id: String,
    pub title: Option<String>,
    #[ts(type = "number | null")]
    pub started_at: Option<u64>,
    pub transcript_path: Option<String>,
    /// Folder to resume in.
    pub directory: Option<String>,
    /// Such as `claude --resume <id>`.
    pub resume_command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ServerStatus {
    Running,
    /// Over the memory threshold or leaking.
    Attention,
    /// Idle for over an hour, or its folder is gone.
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum CleanUpReason {
    WorktreeDeleted,
    Idle {
        #[ts(type = "number")]
        seconds: u64,
    },
    LongRunning {
        #[ts(type = "number")]
        seconds: u64,
    },
    Leaking {
        /// Growth over the history window.
        #[ts(type = "number")]
        bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Alert {
    pub port: u16,
    pub kind: AlertKind,
    #[ts(type = "number")]
    pub memory: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AlertKind {
    OverThreshold,
    Leaking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RequestResult {
    #[ts(type = "number")]
    pub id: u64,
    /// None on success.
    pub error: Option<String>,
}

// ---------------------------------------------------------------- requests (client -> ppm)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Request {
    #[ts(type = "number")]
    pub id: u64,
    #[serde(flatten)]
    pub call: Call,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
#[ts(export)]
pub enum Call {
    /// Send a fresh snapshot now.
    Refresh,
    /// SIGTERM the server's tree (SIGKILL with `force`). Checks every ProcRef first.
    Stop {
        port: u16,
        root: ProcRef,
        force: bool,
    },
    /// Stop, then run `command` again in `launch_dir`.
    Restart { port: u16, root: ProcRef },
    /// Replace the scanner config for this connection.
    Configure(Config),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Config {
    pub min_port: u16,
    pub max_port: u16,
    /// Scan interval.
    #[ts(type = "number")]
    pub interval_ms: u64,
    /// `attention` and alerts above this.
    #[ts(type = "number")]
    pub alert_memory: u64,
    /// Growth over the history window that counts as leaking.
    #[ts(type = "number")]
    pub leak_growth: u64,
    #[ts(type = "number")]
    pub idle_after_secs: u64,
    #[ts(type = "number")]
    pub long_running_after_secs: u64,
    /// Process names clean up never suggests, such as `postgres`.
    pub protected: Vec<String>,
}

impl Default for Config {
    /// WhatThePort's defaults.
    fn default() -> Self {
        const MB: u64 = 1024 * 1024;
        Self {
            min_port: 3000,
            max_port: 65535,
            interval_ms: 2000,
            alert_memory: 2048 * MB,
            leak_growth: 500 * MB,
            idle_after_secs: 4 * 3600,
            long_running_after_secs: 3 * 86400,
            protected: ["postgres", "redis-server", "mongod", "mysqld", "mysql"]
                .map(String::from)
                .to_vec(),
        }
    }
}
