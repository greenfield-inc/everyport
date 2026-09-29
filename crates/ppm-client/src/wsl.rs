//! WSL distros on a Windows machine. Each one is its own machine, reached with
//! `wsl.exe -d <distro> --exec`, and ports Windows sees through WSL's relay
//! show once, under their distro.

use ppm_core::protocol::Snapshot;

/// The command prefix for a distro. `wsl.exe` runs directly, never through
/// `cmd.exe` or PowerShell.
pub fn prefix(distro: &str) -> Vec<String> {
    ["wsl.exe", "-d", distro, "--exec"]
        .map(String::from)
        .to_vec()
}

/// Installed distros, from `wsl.exe -l -q`. Empty when WSL isn't installed or
/// this isn't Windows.
pub async fn distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut command = crate::remote::command(
        &[],
        ppm_core::protocol::Os::Windows,
        &["wsl.exe", "-l", "-q"],
    );
    command.env("WSL_UTF8", "1");
    match command.output().await {
        Ok(out) if out.status.success() => parse_list(&out.stdout),
        _ => Vec::new(),
    }
}

/// `wsl.exe -l -q` prints one distro per line in UTF-16LE, or UTF-8 when
/// `WSL_UTF8=1` is honored. Docker Desktop's own distros hold no servers.
fn parse_list(stdout: &[u8]) -> Vec<String> {
    let utf16 = stdout.len() >= 2 && stdout.len().is_multiple_of(2) && stdout[1] == 0;
    let text = if utf16 {
        let units: Vec<u16> = stdout
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(stdout).into_owned()
    };
    text.lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(String::from)
        .collect()
}

/// Removes the servers from a Windows snapshot that a WSL distro also
/// reports, so each shows once, under its distro with its Linux process tree.
///
/// Matches by port. On Windows the listener is `wslrelay.exe` (NAT
/// networking) or, with mirrored networking, possibly a `svchost` that no name
/// check can tell apart, so the port is what ties the two together.
pub fn dedupe(windows: &mut Snapshot, distros: &[&Snapshot]) {
    windows.servers.retain(|server| {
        !distros
            .iter()
            .any(|d| d.servers.iter().any(|s| s.port == server.port))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../packages/protocol/fixtures/snapshot.json"
        ))
        .unwrap()
    }

    #[test]
    fn reads_utf16_and_utf8_distro_lists() {
        // `wsl.exe -l -q` on a machine with Ubuntu and Docker Desktop.
        let text = "Ubuntu-22.04\r\ndocker-desktop\r\nDebian\r\n";
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(parse_list(&utf16), ["Ubuntu-22.04", "Debian"]);
        assert_eq!(parse_list(text.as_bytes()), ["Ubuntu-22.04", "Debian"]);
        assert_eq!(parse_list(b""), Vec::<String>::new());
    }

    #[test]
    fn a_port_a_distro_serves_shows_only_under_the_distro() {
        let mut windows = fixture();
        let mut ubuntu = fixture();
        ubuntu.servers.retain(|s| s.port == 3000);
        let empty = Snapshot {
            servers: Vec::new(),
            ..fixture()
        };

        dedupe(&mut windows, &[&empty, &ubuntu]);
        let left: Vec<u16> = windows.servers.iter().map(|s| s.port).collect();
        assert_eq!(left, [3001, 5173, 6006, 8000]);
    }
}
