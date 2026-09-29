//! The OS seam. Each OS implements `Platform`; everything above it is shared.
//! Prefer `sysinfo` and `listeners` for the common path and add OS-specific
//! code only where they are wrong or slow (see docs/architecture.md).

use crate::protocol::ProcRef;
use std::collections::HashMap;
use std::io;
use std::net::IpAddr;

/// A TCP socket in LISTEN state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub address: String,
}

/// A TCP socket in LISTEN state held by another user or the system, whose
/// process we may not inspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherListener {
    pub port: u16,
    pub address: String,
    /// User name, such as `root`.
    pub owner: Option<String>,
    /// When the OS says which process holds the socket.
    pub pid: Option<u32>,
    /// The process's name, when the OS gives it but everyport can't list the process.
    pub process_name: Option<String>,
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
    /// Sockets of processes we may inspect: our user's, or every one as root.
    fn listeners(&self) -> io::Result<Vec<Listener>>;
    /// The listening sockets `listeners` leaves out, where the OS shows them.
    fn other_listeners(&self) -> io::Result<Vec<OtherListener>> {
        Ok(Vec::new())
    }
    fn processes(&self) -> io::Result<Vec<ProcInfo>>;
    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails>;
    fn usage(&self, pid: u32) -> Option<ProcUsage>;
    fn memory(&self) -> MemoryStats;
    /// Whole-machine CPU, 0-100, since the previous call.
    fn cpu_percent(&self) -> f32;
    /// Terminate (or kill, with `force`) one process. Must return an error,
    /// without signalling, when `target.started_at` no longer matches.
    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()>;
    /// Asks a whole tree to exit the way its terminal would, for a platform
    /// whose terminate isn't graceful (Ctrl+C on a Windows console). `tree`
    /// is every process being stopped. Returns the ones it reached: the
    /// engine terminates only the rest, and kills what is left after its
    /// grace period.
    fn interrupt(&self, _tree: &[ProcRef]) -> Vec<ProcRef> {
        Vec::new()
    }
    /// Established inbound TCP connections, counted by local port, or `None`
    /// when unknown. The engine uses them to tell an idle server from one in
    /// use, and never calls a server idle while they are unknown.
    fn connections(&self) -> Option<HashMap<u16, u32>> {
        None
    }
    /// The full environment of a process, which `restart` launches with.
    fn environment(&self, _pid: u32) -> Option<Vec<(String, String)>> {
        None
    }
    /// Unix milliseconds. Only a test platform overrides it.
    fn now_ms(&self) -> u64 {
        crate::now_ms()
    }
}

/// True when a Windows listener belongs to WSL, so its port shows under the
/// distro instead of as a Windows server. Pass the listener's process name.
///
/// - NAT networking (the default): `wslrelay.exe` listens on Windows for each
///   port a Linux process binds, and relays the traffic into the VM. Inbox
///   WSL on older Windows 10 builds relays through `wslhost.exe` instead.
/// - Mirrored networking: `wslservice.exe` reserves the port through the Host
///   Network Service. If Windows lists an owner at all, it is `wslservice.exe`
///   or an HNS `svchost.exe`. The latter can't be told apart from other
///   services by name, so match those ports against the distro's instead.
pub fn is_wsl_owner(process_name: &str) -> bool {
    let name = process_name.to_ascii_lowercase();
    matches!(
        name.strip_suffix(".exe").unwrap_or(&name),
        "wslrelay" | "wslhost" | "wslservice"
    )
}

/// A TCP socket's local address and the processes holding it. `None` when
/// they are unknown.
struct Socket {
    address: IpAddr,
    port: u16,
    holders: Option<Vec<u32>>,
}

/// Counts established TCP sockets by local port, keeping the ones a listener
/// accepted: on its port, at its address or under an unspecified one, and
/// held by a process holding the listener or started under one. An outgoing
/// connection can use the same port number, on another address or, under a
/// wildcard listener, from another process. A socket or listener whose
/// holders are unknown is counted on its address alone, so a server in use
/// never looks idle. `parent` gives a process's parent.
fn inbound_connections(
    listening: &[Socket],
    established: impl IntoIterator<Item = Socket>,
    mut parent: impl FnMut(u32) -> Option<u32>,
) -> HashMap<u16, u32> {
    let mut counts = HashMap::new();
    for socket in established {
        let address = socket.address.to_canonical();
        let listeners: Vec<&Socket> = listening
            .iter()
            .filter(|l| {
                l.port == socket.port
                    && (l.address.is_unspecified() || l.address.to_canonical() == address)
            })
            .collect();
        let listener_pids: Option<Vec<u32>> = listeners
            .iter()
            .map(|l| l.holders.clone())
            .collect::<Option<Vec<_>>>()
            .map(|pids| pids.concat());
        let accepted = !listeners.is_empty()
            && match (&socket.holders, listener_pids) {
                (Some(holders), Some(roots)) => holders
                    .iter()
                    .any(|&holder| in_tree(holder, &roots, &mut parent)),
                _ => true,
            };
        if accepted {
            *counts.entry(socket.port).or_default() += 1;
        }
    }
    counts
}

/// True when `pid` is one of `roots` or descends from one.
fn in_tree(pid: u32, roots: &[u32], parent: &mut impl FnMut(u32) -> Option<u32>) -> bool {
    let mut seen = Vec::new();
    let mut current = Some(pid);
    while let Some(pid) = current.filter(|pid| !seen.contains(pid)) {
        if roots.contains(&pid) {
            return true;
        }
        seen.push(pid);
        current = parent(pid);
    }
    false
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
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

/// Runs this process as a platform helper, and exits, when everyport started it as
/// one. A binary that stops servers through `native()` calls it first in `main`.
pub fn run_helper() {
    #[cfg(windows)]
    windows::run_helper();
}

#[cfg(test)]
mod tests {
    use super::is_wsl_owner;

    #[test]
    fn wsl_owners_are_the_relays_and_the_service() {
        for name in [
            "wslrelay.exe",
            "wslhost.exe",
            "WslService.exe",
            "WSLRELAY.EXE",
        ] {
            assert!(is_wsl_owner(name), "{name}");
        }
        for name in ["node.exe", "svchost.exe", "wsl.exe", "relay.exe", ""] {
            assert!(!is_wsl_owner(name), "{name}");
        }
    }
}
