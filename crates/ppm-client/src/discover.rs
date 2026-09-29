//! Machines the user can add without typing a command: hosts in
//! `~/.ssh/config`, Pane remote hosts, and WSL distros on Windows.

use crate::machines::{Machine, Via};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    SshConfig,
    Pane,
    Wsl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub machine: Machine,
    pub source: Source,
}

/// Everything found on this machine, in source order.
pub async fn all() -> Vec<Found> {
    let home = dirs::home_dir().unwrap_or_default();
    let pane_dir = std::env::var_os("PANE_DIR").map_or_else(|| home.join(".pane"), PathBuf::from);
    let found = |source| move |machine| Found { machine, source };
    ssh_config(&home.join(".ssh").join("config"))
        .into_iter()
        .map(found(Source::SshConfig))
        .chain(pane(&pane_dir).into_iter().map(found(Source::Pane)))
        .chain(
            crate::wsl::distros()
                .await
                .into_iter()
                .map(|distro| Machine {
                    name: distro.clone(),
                    via: Via::Command {
                        command: crate::wsl::prefix(&distro),
                    },
                })
                .map(found(Source::Wsl)),
        )
        .collect()
}

/// Named `Host` entries in an ssh config file and the files it `Include`s.
/// Patterns with wildcards or negation aren't machines, so they're skipped.
pub fn ssh_config(path: &Path) -> Vec<Machine> {
    let mut hosts = Vec::new();
    read_ssh_config(path, path.parent().unwrap_or(Path::new(".")), 0, &mut hosts);
    hosts
        .into_iter()
        .map(|host| Machine {
            name: host.clone(),
            via: Via::Command {
                command: vec!["ssh".into(), host],
            },
        })
        .collect()
}

fn read_ssh_config(path: &Path, ssh_dir: &Path, depth: u32, hosts: &mut Vec<String>) {
    // ssh itself stops at 16 levels of Include.
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    if depth > 16 {
        return;
    }
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
                for host in values {
                    let pattern = host.contains(['*', '?']) || host.starts_with('!');
                    if !pattern && !hosts.contains(&host) {
                        hosts.push(host);
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
        let dir = std::env::temp_dir().join(format!("ppm-discover-{name}-{}", std::process::id()));
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

        let machines = ssh_config(&dir.join("config"));
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
}
