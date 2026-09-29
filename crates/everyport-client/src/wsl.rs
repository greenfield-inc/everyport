//! WSL distros on a Windows machine. Each one is its own machine, reached with
//! `wsl.exe -d <distro> --exec`, and ports Windows sees through WSL's relay
//! show once, under their distro.

use everyport_core::platform::is_wsl_owner;
use everyport_core::protocol::Snapshot;

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
        everyport_core::protocol::Os::Windows,
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
    let text = match stdout {
        [0xFF, 0xFE, rest @ ..] => utf16le(rest),
        [_, 0, ..] => utf16le(stdout),
        _ => String::from_utf8_lossy(stdout).into_owned(),
    };
    text.lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(String::from)
        .collect()
}

fn utf16le(bytes: &[u8]) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
    String::from_utf16_lossy(&units)
}

/// Removes the ports from a Windows snapshot that WSL relays for a distro,
/// so each shows once, under its distro with its Linux process tree.
///
/// A Windows port goes when a distro reports it, as a server or as another
/// user's port, and its owner is a WSL process, or `svchost`, which can own
/// the port in mirrored networking.
/// A Windows program that shares a port with a distro stays.
pub fn dedupe(windows: &mut Snapshot, distros: &[&Snapshot]) {
    let relayed = |name: &str, port: u16| {
        let name = name.to_ascii_lowercase();
        (is_wsl_owner(&name) || name.strip_suffix(".exe").unwrap_or(&name) == "svchost")
            && distros.iter().any(|d| {
                d.servers.iter().any(|s| s.port == port)
                    || d.other_ports.iter().any(|o| o.port == port)
            })
    };
    windows
        .servers
        .retain(|server| !relayed(&server.process_name, server.port));
    windows.other_ports.retain(|other| {
        !other
            .process_name
            .as_deref()
            .is_some_and(|name| relayed(name, other.port))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use everyport_core::protocol::OtherPort;

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
        let with_bom = [&[0xFF, 0xFE][..], &utf16].concat();
        assert_eq!(parse_list(&with_bom), ["Ubuntu-22.04", "Debian"]);
        assert_eq!(parse_list(text.as_bytes()), ["Ubuntu-22.04", "Debian"]);
        assert_eq!(parse_list(b""), Vec::<String>::new());
    }

    #[test]
    fn a_relayed_port_shows_only_under_its_distro() {
        // Windows sees Ubuntu's :3000 through wslrelay.exe and :5173 through
        // svchost (mirrored networking). Its own node.exe on :3001 shares a
        // port with Ubuntu, and :6006 is relayed for no connected distro.
        // Mirrored networking's svchost can also run as a service account,
        // so :5173 can come as another user's port too. Ubuntu's root runs
        // :8000, which Windows relays like any other.
        let mut windows = fixture();
        windows.other_ports.push(OtherPort {
            port: 5173,
            addresses: vec!["0.0.0.0".into()],
            owner: Some("SYSTEM".into()),
            process_name: Some("svchost.exe".into()),
        });
        for (port, owner) in [
            (3000, "wslrelay.exe"),
            (3001, "node.exe"),
            (5173, "svchost.exe"),
            (6006, "wslrelay.exe"),
            (8000, "wslrelay.exe"),
        ] {
            windows
                .servers
                .iter_mut()
                .find(|s| s.port == port)
                .unwrap()
                .process_name = owner.into();
        }
        let mut ubuntu = fixture();
        ubuntu
            .servers
            .retain(|s| [3000, 3001, 5173].contains(&s.port));
        ubuntu.other_ports.push(OtherPort {
            port: 8000,
            addresses: vec!["0.0.0.0".into()],
            owner: Some("root".into()),
            process_name: None,
        });
        let empty = Snapshot {
            servers: Vec::new(),
            ..fixture()
        };

        dedupe(&mut windows, &[&empty, &ubuntu]);
        let left: Vec<u16> = windows.servers.iter().map(|s| s.port).collect();
        assert_eq!(left, [3001, 6006]);
        let others: Vec<u16> = windows.other_ports.iter().map(|o| o.port).collect();
        assert_eq!(others, [631, 5432]);
    }
}
