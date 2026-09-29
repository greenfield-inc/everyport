use crate::protocol::{HostInfo, Os};

pub fn info() -> HostInfo {
    HostInfo {
        hostname: hostname(),
        os: if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        },
        arch: std::env::consts::ARCH.to_string(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
    }
}

/// zsh, the macOS default shell, doesn't export `HOSTNAME`, so fall back to
/// the kernel's name on Linux and the `hostname` command elsewhere.
fn hostname() -> String {
    let from_command = || {
        let output = std::process::Command::new("hostname").output().ok()?;
        String::from_utf8(output.stdout).ok()
    };
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .or_else(from_command)
        .map(|name| name.trim().to_string())
        .unwrap_or_default()
}
