//! Makes a remote server's port reachable on this machine, so its URL opens
//! in the local browser.

use crate::{remote, Connection};
use anyhow::{bail, Context};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::process::Stdio;
use std::time::Duration;

/// A forwarded port. Dropping it closes the tunnel.
#[derive(Debug)]
pub struct Forward {
    /// The URL to open, such as `http://127.0.0.1:5173`.
    pub url: String,
    pub local_port: u16,
    _tunnel: Option<tokio::process::Child>,
}

impl Forward {
    /// The server is reachable here as it is, at `localhost`, which reaches
    /// servers bound to `::1` as well as `127.0.0.1`.
    fn direct(port: u16) -> Self {
        Self {
            url: format!("http://localhost:{port}"),
            local_port: port,
            _tunnel: None,
        }
    }
}

/// Forwards `port` on the machine behind `connection` to this machine. Uses
/// the same port number when it's free here. This machine and WSL distros
/// (WSL forwards `localhost` itself) need no tunnel.
pub async fn forward(connection: &Connection, port: u16) -> anyhow::Result<Forward> {
    let prefix = match connection {
        Connection::Sidecar { .. } => return Ok(Forward::direct(port)),
        Connection::Command { argv_prefix, .. } if remote::program(argv_prefix) == "wsl" => {
            return Ok(Forward::direct(port))
        }
        Connection::Command { argv_prefix, .. } => argv_prefix,
        Connection::Http { url, .. } => {
            bail!(
                "Can't forward ports from {url}. Forward port {port} through your tunnel or proxy."
            )
        }
    };
    let local_port = free_port(port)?;
    let argv = tunnel(prefix, local_port, port)?;
    let mut command = tokio::process::Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(remote::CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .with_context(|| format!("Couldn't run {}", argv[0]))?;
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            let out = child.wait_with_output().await?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "Couldn't forward port {port}: {}",
                if stderr.trim().is_empty() {
                    status.to_string()
                } else {
                    stderr.trim().to_string()
                }
            );
        }
        if TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).is_ok() {
            // The tunnel listens on 127.0.0.1 only. `localhost` can resolve
            // to ::1 first and reach a different local server.
            return Ok(Forward {
                url: format!("http://127.0.0.1:{local_port}"),
                local_port,
                _tunnel: Some(child),
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("Forwarding port {port} took longer than 10 s.")
}

/// `port` when no local server answers on it at 127.0.0.1 or ::1, otherwise
/// any free port.
fn free_port(port: u16) -> anyhow::Result<u16> {
    let answers = |ip: IpAddr| {
        TcpStream::connect_timeout(&(ip, port).into(), Duration::from_millis(200)).is_ok()
    };
    let taken = answers(Ipv4Addr::LOCALHOST.into()) || answers(Ipv6Addr::LOCALHOST.into());
    if !taken && TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() {
        return Ok(port);
    }
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port())
}

/// The command that forwards `remote` to `local` for a connection prefix.
fn tunnel(prefix: &[String], local: u16, remote: u16) -> anyhow::Result<Vec<String>> {
    match remote::program(prefix).as_str() {
        "ssh" => {
            let spec = format!("127.0.0.1:{local}:localhost:{remote}");
            let mut options = remote::SSH_KEEPALIVE.to_vec();
            options.extend(["-N", "-o", "ExitOnForwardFailure=yes", "-L", &spec]);
            Ok(remote::ssh_with(prefix, &options))
        }
        "kubectl" => kubectl_port_forward(prefix, local, remote),
        program => bail!(
            "Opening a server's URL needs an ssh or kubectl connection. {program} can't forward ports, so publish port {remote} yourself (for Docker, `docker run -p {remote}:{remote}`)."
        ),
    }
}

/// `kubectl [global flags] exec -i <pod> [-n ns] [-c ctr] --` becomes
/// `kubectl [global flags] port-forward --address 127.0.0.1 [-n ns] <pod> <local>:<remote>`.
fn kubectl_port_forward(prefix: &[String], local: u16, remote: u16) -> anyhow::Result<Vec<String>> {
    let exec = prefix
        .iter()
        .position(|a| a == "exec")
        .context("A kubectl connection runs `kubectl exec`.")?;
    let mut argv = prefix[..exec].to_vec();
    argv.extend(["port-forward", "--address", "127.0.0.1"].map(String::from));
    let mut pod = None;
    let mut args = prefix[exec + 1..].iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => break,
            "-n" | "--namespace" => {
                argv.extend([arg.clone(), args.next().cloned().unwrap_or_default()])
            }
            "-c" | "--container" => {
                args.next();
            }
            a if a.starts_with("--namespace=") => argv.push(arg.clone()),
            a if a.starts_with('-') => {}
            _ if pod.is_none() => pod = Some(arg.clone()),
            _ => {}
        }
    }
    argv.push(pod.context("The kubectl connection names no pod.")?);
    argv.push(format!("{local}:{remote}"));
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ssh_forwards_with_dash_l() {
        assert_eq!(
            tunnel(&strings(&["ssh", "-p", "2222", "me@devbox"]), 3000, 3000).unwrap(),
            strings(&[
                "ssh",
                "-o",
                "ServerAliveInterval=15",
                "-N",
                "-o",
                "ExitOnForwardFailure=yes",
                "-L",
                "127.0.0.1:3000:localhost:3000",
                "-p",
                "2222",
                "me@devbox"
            ])
        );
    }

    #[test]
    fn kubectl_exec_becomes_port_forward() {
        // https://kubernetes.io/docs/reference/kubectl/generated/kubectl_port-forward/
        assert_eq!(
            tunnel(
                &strings(&[
                    "kubectl",
                    "--context",
                    "prod",
                    "exec",
                    "-i",
                    "-n",
                    "web",
                    "api-7d9f",
                    "-c",
                    "app",
                    "--"
                ]),
                5173,
                3000
            )
            .unwrap(),
            strings(&[
                "kubectl",
                "--context",
                "prod",
                "port-forward",
                "--address",
                "127.0.0.1",
                "-n",
                "web",
                "api-7d9f",
                "5173:3000"
            ])
        );
        assert_eq!(
            tunnel(
                &strings(&["kubectl", "exec", "-i", "deploy/web", "--"]),
                3000,
                3000
            )
            .unwrap(),
            strings(&[
                "kubectl",
                "port-forward",
                "--address",
                "127.0.0.1",
                "deploy/web",
                "3000:3000"
            ])
        );
    }

    #[test]
    fn other_prefixes_explain_why_they_cannot_forward() {
        let error = tunnel(&strings(&["docker", "exec", "-i", "box"]), 3000, 3000)
            .unwrap_err()
            .to_string();
        assert!(error.contains("docker can't forward ports"), "{error}");
    }
}
