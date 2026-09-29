//! The OS seam. Each OS implements `Platform`; everything above it is shared.
//! Prefer `sysinfo` and `listeners` for the common path and add OS-specific
//! code only where they are wrong or slow (see docs/architecture.md).

use crate::protocol::ProcRef;
use std::io;

/// A TCP socket in LISTEN state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub address: String,
}

/// Cheap per-process facts, read for every process on every scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub proc: ProcRef,
    pub parent: Option<u32>,
    pub name: String,
}

/// Facts read only for processes behind a listening port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcDetails {
    pub cwd: Option<String>,
    pub args: Vec<String>,
    /// Only the variables the engine asks for, such as agent session ids.
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcUsage {
    /// Physical footprint, matching Activity Monitor / Task Manager.
    pub memory: u64,
    /// Total CPU time used so far; the engine diffs it between scans.
    pub cpu_time_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryStats {
    pub total: u64,
    pub used: u64,
}

pub trait Platform: Send + Sync {
    fn listeners(&self) -> io::Result<Vec<Listener>>;
    fn processes(&self) -> io::Result<Vec<ProcInfo>>;
    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails>;
    fn usage(&self, pid: u32) -> Option<ProcUsage>;
    fn memory(&self) -> MemoryStats;
    /// Whole-machine CPU, 0-100, since the previous call.
    fn cpu_percent(&self) -> f32;
    /// Terminate (or kill, with `force`) one process. Must return an error,
    /// without signalling, when `target.started_at` no longer matches.
    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// The platform for the OS this binary runs on.
pub fn native() -> Box<dyn Platform> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::Macos::new());
    #[cfg(target_os = "linux")]
    return Box::new(linux::Linux::new());
    #[cfg(windows)]
    return Box::new(windows::Windows::new());
}
