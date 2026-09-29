//! Machines the user can add without typing a command: hosts in
//! `~/.ssh/config`, Pane remote hosts, Tailscale peers, and WSL distros on
//! Windows.

use crate::client::check::Hint;
use crate::client::machines::{Machine, Via};
use crate::protocol::Os;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    SshConfig,
    Pane,
    Tailscale,
    Wsl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub machine: Machine,
    pub source: Source,
    /// Whether Pane lists it, and its OS when Tailscale reports one, from
    /// every source that found the same host.
    pub hint: Hint,
    /// Lowercase names of the host it reaches: the ssh destination, its
    /// `HostName`, and Tailscale's names and IPs.
    names: Vec<String>,
}

impl Found {
    pub fn new(machine: Machine, source: Source) -> Self {
        Self {
            names: names_of(&machine.via),
            hint: Hint {
                os: None,
                pane: source == Source::Pane,
            },
            machine,
            source,
        }
    }

    /// Where it came from, such as `Tailscale (Windows)`.
    pub fn label(&self) -> String {
        let os = match self.hint.os {
            Some(Os::Macos) => "macOS",
            Some(Os::Linux) => "Linux",
            Some(Os::Windows) => "Windows",
            None => "",
        };
        match self.source {
            Source::SshConfig => "SSH config".into(),
            Source::Pane => "Pane".into(),
            Source::Tailscale => format!("Tailscale ({os})"),
            Source::Wsl => "WSL".into(),
        }
    }

    fn same_host(&self, names: &[String]) -> bool {
        self.names.iter().any(|name| names.contains(name))
    }
}

/// Lowercase names of the host behind `via`: the ssh destination without
/// its user, or the URL's host.
fn names_of(via: &Via) -> Vec<String> {
    let host = match via {
        Via::Command { command } if command.first().is_some_and(|c| c == "ssh") => command
            .last()
            .map(|dest| dest.rsplit('@').next().unwrap_or(dest)),
        Via::Url { url, .. } => Some(url_host(url)),
        Via::Command { .. } => None,
    };
    host.filter(|h| !h.is_empty())
        .map(|h| vec![h.to_ascii_lowercase()])
        .unwrap_or_default()
}

/// Everything found on this machine, in source order. Sources that find the
/// same host become one entry, the first, which keeps what the others knew.
pub async fn all() -> Vec<Found> {
    let home = dirs::home_dir().unwrap_or_default();
    let pane_dir = std::env::var_os("PANE_DIR").map_or_else(|| home.join(".pane"), PathBuf::from);
    let mut found = ssh_config(&home.join(".ssh").join("config"));
    found.extend(
        pane(&pane_dir)
            .into_iter()
            .map(|m| Found::new(m, Source::Pane)),
    );
    if let Some(status) = tailscale_status().await {
        found.extend(tailscale(&status));
    }
    found.extend(
        crate::client::wsl::distros()
            .await
            .into_iter()
            .map(|distro| {
                let machine = Machine {
                    name: distro.clone(),
                    via: Via::Command {
                        command: crate::client::wsl::prefix(&distro),
                    },
                };
                Found::new(machine, Source::Wsl)
            }),
    );
    merge(found)
}

/// Folds each entry into an earlier one for the same host.
fn merge(found: Vec<Found>) -> Vec<Found> {
    let mut kept: Vec<Found> = Vec::new();
    for f in found {
        match kept.iter_mut().find(|k| k.same_host(&f.names)) {
            Some(k) => {
                k.hint.os = k.hint.os.or(f.hint.os);
                k.hint.pane |= f.hint.pane;
                k.names.extend(f.names);
            }
            None => kept.push(f),
        }
    }
    kept
}

/// The found machines that aren't saved already, by name, connection or host.
pub fn unsaved(saved: &[Machine], found: &[Found]) -> Vec<Found> {
    found
        .iter()
        .filter(|f| {
            !saved.iter().any(|m| {
                m.name == f.machine.name || m.via == f.machine.via || f.same_host(&names_of(&m.via))
            })
        })
        .cloned()
        .collect()
}

/// What discovery knows about the machine reached through `via`, from the
/// found entry for the same connection or host.
pub fn hint(found: &[Found], via: &Via) -> Hint {
    let names = names_of(via);
    found
        .iter()
        .find(|f| f.machine.via == *via || f.same_host(&names))
        .map(|f| f.hint.clone())
        .unwrap_or_default()
}

/// `tailscale status --json`, from the first `tailscale` CLI found.
async fn tailscale_status() -> Option<String> {
    const MAC_APP: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
    let clis = [
        "tailscale",
        MAC_APP,
        r"C:\Program Files\Tailscale\tailscale.exe",
    ];
    for cli in clis {
        // The Mac app's binary is also its GUI, so it's asked only while the
        // app already runs.
        if cli == MAC_APP && !mac_app_running() {
            continue;
        }
        let mut command = tokio::process::Command::new(cli);
        command
            .args(["status", "--json"])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(crate::client::remote::CREATE_NO_WINDOW);
        match tokio::time::timeout(Duration::from_secs(3), command.output()).await {
            Ok(Ok(out)) if out.status.success() => {
                return Some(String::from_utf8_lossy(&out.stdout).into_owned())
            }
            // It's there but not running or signed in.
            Ok(Ok(_)) | Err(_) => return None,
            Ok(Err(_)) => {}
        }
    }
    None
}

fn mac_app_running() -> bool {
    cfg!(target_os = "macos")
        && std::process::Command::new("pgrep")
            .args(["-xq", "Tailscale"])
            .status()
            .is_ok_and(|s| s.success())
}

/// Online peers in `tailscale status --json` that can run everyport, as ssh
/// machines named by their MagicDNS name. Skips this device.
pub fn tailscale(status: &str) -> Vec<Found> {
    #[derive(Deserialize)]
    struct Status {
        #[serde(rename = "Self")]
        this: Option<Peer>,
        #[serde(rename = "Peer", default)]
        peers: HashMap<String, Peer>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Peer {
        #[serde(rename = "DNSName", default)]
        dns_name: String,
        #[serde(rename = "OS", default)]
        os: String,
        #[serde(default)]
        online: bool,
        #[serde(rename = "TailscaleIPs", default)]
        tailscale_ips: Vec<String>,
    }

    let Ok(status) = serde_json::from_str::<Status>(status) else {
        return Vec::new();
    };
    let this = status.this.map(|s| s.dns_name).unwrap_or_default();
    let mut peers: Vec<Found> = status
        .peers
        .into_values()
        .filter(|peer| peer.online && !peer.dns_name.is_empty() && peer.dns_name != this)
        .filter_map(|peer| {
            let os = match peer.os.as_str() {
                "macOS" => Os::Macos,
                "linux" => Os::Linux,
                "windows" => Os::Windows,
                // Phones and TVs can't run everyport.
                _ => return None,
            };
            let name = peer.dns_name.trim_end_matches('.').to_ascii_lowercase();
            let short = name.split('.').next().unwrap_or(&name).to_string();
            let mut found = Found::new(
                Machine {
                    name: name.clone(),
                    via: Via::Command {
                        command: vec!["ssh".into(), name],
                    },
                },
                Source::Tailscale,
            );
            found.hint.os = Some(os);
            found.names.push(short);
            found.names.extend(peer.tailscale_ips);
            Some(found)
        })
        .collect();
    peers.sort_by(|a, b| a.machine.name.cmp(&b.machine.name));
    peers
}

/// Named `Host` entries in an ssh config file and the files it `Include`s,
/// with the `HostName` each one connects to. Patterns with wildcards or
/// negation aren't machines, so they're skipped.
pub fn ssh_config(path: &Path) -> Vec<Found> {
    let mut hosts = Vec::new();
    read_ssh_config(path, path.parent().unwrap_or(Path::new(".")), 0, &mut hosts);
    hosts
        .into_iter()
        .map(|(host, hostname)| {
            let mut found = Found::new(
                Machine {
                    name: host.clone(),
                    via: Via::Command {
                        command: vec!["ssh".into(), host],
                    },
                },
                Source::SshConfig,
            );
            found.names.extend(hostname.map(|h| h.to_ascii_lowercase()));
            found
        })
        .collect()
}

/// Appends each named host and, like ssh, the first `HostName` given for it.
fn read_ssh_config(
    path: &Path,
    ssh_dir: &Path,
    depth: u32,
    hosts: &mut Vec<(String, Option<String>)>,
) {
    // ssh itself stops at 16 levels of Include.
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    if depth > 16 {
        return;
    }
    // The named hosts of the current `Host` block.
    let mut block: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let (keyword, rest) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or((line, ""));
        let values = words(rest.trim_start_matches(|c: char| c.is_whitespace() || c == '='));
        match keyword.to_ascii_lowercase().as_str() {
            "host" => {
                block.clear();
                for host in values {
                    let pattern = host.contains(['*', '?']) || host.starts_with('!');
                    if pattern {
                        continue;
                    }
                    if !hosts.iter().any(|(h, _)| *h == host) {
                        hosts.push((host.clone(), None));
                    }
                    block.push(host);
                }
            }
            "match" => block.clear(),
            "hostname" => {
                for (host, hostname) in hosts.iter_mut() {
                    if hostname.is_none() && block.contains(host) {
                        *hostname = values.first().cloned();
                    }
                }
            }
            "include" => {
                for include in values {
                    for file in expand(&include, ssh_dir) {
                        read_ssh_config(&file, ssh_dir, depth + 1, hosts);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Splits ssh config arguments on whitespace, keeping "double quoted" words.
fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (word, tail) = match rest.strip_prefix('"') {
            Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
            None => rest.split_once(char::is_whitespace).unwrap_or((rest, "")),
        };
        if word.starts_with('#') {
            break;
        }
        words.push(word.to_string());
        rest = tail.trim_start();
    }
    words
}

/// An `Include` argument as files: `~` is home, relative paths are under
/// `~/.ssh`, and `*` or `?` in the file name match files in its folder.
fn expand(include: &str, ssh_dir: &Path) -> Vec<PathBuf> {
    let path = match include.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => ssh_dir.join(include),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let dir = path.parent().unwrap_or(ssh_dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            matches(
                name.as_bytes(),
                entry.file_name().to_string_lossy().as_bytes(),
            )
        })
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

/// Glob match for `*` and `?`.
fn matches(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            matches(&pattern[1..], name) || (!name.is_empty() && matches(pattern, &name[1..]))
        }
        (Some(b'?'), Some(_)) => matches(&pattern[1..], &name[1..]),
        (Some(p), Some(n)) if p == n => matches(&pattern[1..], &name[1..]),
        _ => false,
    }
}

/// Pane remote hosts, from `config.json` in the Pane folder, as ssh machines.
/// Reads only the label, URL and tunnel of each profile, never its token.
pub fn pane(pane_dir: &Path) -> Vec<Machine> {
    #[derive(Deserialize)]
    struct Config {
        #[serde(rename = "remoteDaemon")]
        remote_daemon: Option<RemoteDaemon>,
    }
    #[derive(Deserialize)]
    struct RemoteDaemon {
        client: Option<RemoteClient>,
    }
    #[derive(Deserialize)]
    struct RemoteClient {
        #[serde(default)]
        profiles: Vec<Profile>,
    }
    #[derive(Deserialize)]
    struct Profile {
        label: String,
        #[serde(rename = "baseUrl")]
        base_url: String,
        tunnel: Option<Tunnel>,
    }
    #[derive(Deserialize)]
    struct Tunnel {
        kind: String,
        command: Option<String>,
        #[serde(rename = "tailscaleIp")]
        tailscale_ip: Option<String>,
    }

    let Ok(text) = std::fs::read_to_string(pane_dir.join("config.json")) else {
        return Vec::new();
    };
    let Ok(config) = serde_json::from_str::<Config>(&text) else {
        return Vec::new();
    };
    let profiles = config
        .remote_daemon
        .and_then(|r| r.client)
        .map(|c| c.profiles)
        .unwrap_or_default();
    profiles
        .into_iter()
        .filter_map(|profile| {
            let tunnel = profile.tunnel.as_ref();
            // Pane's ssh tunnel is `ssh -N -L <port>:127.0.0.1:<port> <user>@<host>`.
            let from_tunnel = tunnel
                .filter(|t| t.kind == "ssh")
                .and_then(|t| t.command.as_deref())
                .and_then(|c| c.split_whitespace().last())
                .filter(|dest| !dest.starts_with('-'))
                .map(String::from);
            let host = url_host(&profile.base_url);
            let reachable = match host {
                "localhost" | "127.0.0.1" | "::1" | "[::1]" | "" => {
                    tunnel.and_then(|t| t.tailscale_ip.clone())
                }
                host => Some(host.to_string()),
            };
            let dest = from_tunnel.or(reachable)?;
            Some(Machine {
                name: profile.label,
                via: Via::Command {
                    command: vec!["ssh".into(), dest],
                },
            })
        })
        .collect()
}

/// The host of `scheme://host:port/path`.
fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if authority.starts_with('[') {
        return authority
            .split_once(']')
            .map_or(authority, |(h, _)| &authority[..h.len() + 1]);
    }
    authority.split(':').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("everyport-discover-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(machines: &[Machine]) -> Vec<&str> {
        machines.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn lists_named_ssh_hosts_and_follows_includes() {
        let dir = temp_dir("ssh");
        std::fs::create_dir_all(dir.join("config.d")).unwrap();
        std::fs::write(
            dir.join("config"),
            "# personal\n\
             Include config.d/*\n\
             Host devbox build-vm\n  HostName 10.0.0.5\n  User me\n\
             Host *.internal !bastion gpu-?\n  ProxyJump bastion\n\
             host=\"lab box\"\n\
             Match host devbox exec \"true\"\n\
             Host *\n  ServerAliveInterval 30\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("config.d").join("work"),
            "Host work-vm devbox\n  User work\n",
        )
        .unwrap();
        std::fs::write(dir.join("config.d").join("nested"), "Include extra\n").unwrap();
        std::fs::write(dir.join("extra"), "Host extra-box\n").unwrap();

        let found = ssh_config(&dir.join("config"));
        let machines: Vec<Machine> = found.iter().map(|f| f.machine.clone()).collect();
        assert_eq!(
            names(&machines),
            ["extra-box", "work-vm", "devbox", "build-vm", "lab box"]
        );
        assert_eq!(
            machines[2].via,
            Via::Command {
                command: vec!["ssh".into(), "devbox".into()]
            }
        );
        // A machine saved by its address is the same host as the aliases for it.
        let saved = Machine {
            name: "server".into(),
            via: Via::Command {
                command: vec!["ssh".into(), "me@10.0.0.5".into()],
            },
        };
        let left: Vec<String> = unsaved(&[saved], &found)
            .into_iter()
            .map(|f| f.machine.name)
            .collect();
        assert_eq!(left, ["extra-box", "work-vm", "lab box"]);
        assert!(ssh_config(&dir.join("missing")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn turns_pane_remote_profiles_into_ssh_machines() {
        let dir = temp_dir("pane");
        // Shaped like Pane's RemotePaneConnectionProfile in shared/types/remoteDaemon.ts.
        std::fs::write(
            dir.join("config.json"),
            r#"{"theme": "dark", "remoteDaemon": {"client": {"mode": "remote", "activeProfileId": "a", "profiles": [
                {"id": "a", "label": "Office Mac mini", "baseUrl": "http://127.0.0.1:42137", "token": "SECRET",
                 "transport": "http+sse", "tunnel": {"kind": "ssh", "command": "ssh -N -L 42137:127.0.0.1:42137 me@mini.local", "selected": true}},
                {"id": "b", "label": "VM", "baseUrl": "https://vm.tail1234.ts.net", "token": "SECRET",
                 "transport": "http+sse", "tunnel": {"kind": "tailscale", "selected": true, "tailscaleIp": "100.64.0.7"}},
                {"id": "c", "label": "Tunnel only", "baseUrl": "http://localhost:42137", "token": "SECRET",
                 "transport": "http+sse", "tunnel": {"kind": "tailscale", "selected": true, "tailscaleIp": "100.64.0.9"}},
                {"id": "d", "label": "Unknown", "baseUrl": "http://127.0.0.1:42137", "token": "SECRET", "transport": "http+sse"}
            ]}}}"#,
        )
        .unwrap();
        let ssh = |host: &str| Via::Command {
            command: vec!["ssh".into(), host.into()],
        };
        let machines = pane(&dir);
        assert_eq!(names(&machines), ["Office Mac mini", "VM", "Tunnel only"]);
        assert_eq!(machines[0].via, ssh("me@mini.local"));
        assert_eq!(machines[1].via, ssh("vm.tail1234.ts.net"));
        assert_eq!(machines[2].via, ssh("100.64.0.9"));
        assert!(!format!("{machines:?}").contains("SECRET"));
        assert!(pane(&dir.join("missing")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn merges_tailscale_peers_into_other_sources_for_the_same_host() {
        let ssh = |name: &str, dest: &str| Machine {
            name: name.into(),
            via: Via::Command {
                command: vec!["ssh".into(), dest.into()],
            },
        };
        // An ssh config alias by the short name, and Pane by Tailscale IP.
        let mut found = vec![
            Found::new(ssh("nas", "nas"), Source::SshConfig),
            Found::new(ssh("Studio", "me@100.101.6.6"), Source::Pane),
        ];
        found.extend(tailscale(include_str!("fixtures/tailscale-status.json")));
        let found = merge(found);
        let summary: Vec<(&str, String, Hint)> = found
            .iter()
            .map(|f| (f.machine.name.as_str(), f.label(), f.hint.clone()))
            .collect();
        let known = |os, pane| Hint { os: Some(os), pane };
        assert_eq!(
            summary,
            [
                ("nas", "SSH config".into(), known(Os::Linux, false)),
                ("Studio", "Pane".into(), known(Os::Macos, true)),
                (
                    "build-server.tail0000.ts.net",
                    "Tailscale (Linux)".into(),
                    known(Os::Linux, false)
                ),
                (
                    "desktop-gaming.tail0000.ts.net",
                    "Tailscale (Windows)".into(),
                    known(Os::Windows, false)
                ),
            ]
        );
        // Saved by its short name, the Windows PC isn't offered again, and keeps its OS.
        let saved = ssh("gaming pc", "desktop-gaming");
        let offered: Vec<String> = unsaved(std::slice::from_ref(&saved), &found)
            .into_iter()
            .map(|f| f.machine.name)
            .collect();
        assert_eq!(offered, ["nas", "Studio", "build-server.tail0000.ts.net"]);
        assert_eq!(hint(&found, &saved.via), known(Os::Windows, false));
        assert!(tailscale("not json").is_empty());
    }
}
