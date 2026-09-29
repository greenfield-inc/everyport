use crate::protocol::{HostInfo, Os};

pub fn info() -> HostInfo {
    HostInfo {
        hostname: std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_default(),
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
