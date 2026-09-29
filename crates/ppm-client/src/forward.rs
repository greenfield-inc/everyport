//! Makes a remote server's port reachable on this machine, so its URL opens
//! in the local browser.

use crate::{remote, Connection};
use anyhow::{bail, Context};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// A forwarded port. Dropping it closes the tunnel.
#[derive(Debug)]
pub struct Forward {
    /// The URL to open, such as `http://127.0.0.1:5173`.
    pub url: String,
    pub local_port: u16,
    tunnel: Option<Tunnel>,
}

#[derive(Debug)]
enum Tunnel {
    /// `ssh -L` or `kubectl port-forward`.
    Process { _child: tokio::process::Child },
    /// A local listener that runs `ppm connect <port>` on the machine for
    /// each connection.
    Relay { accept: tokio::task::JoinHandle<()> },
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Tunnel::Relay { accept } = self {
            accept.abort();
        }
    }
}

impl Forward {
    /// The server is reachable here as it is, at `localhost`, which reaches
    /// servers bound to `::1` as well as `127.0.0.1`.
    fn direct(port: u16) -> Self {
        Self {
            url: format!("http://localhost:{port}"),
            local_port: port,
            tunnel: None,
        }
    }

    /// The tunnel listens on 127.0.0.1 only. `localhost` can resolve to ::1
    /// first and reach a different local server.
    fn tunneled(local_port: u16, tunnel: Tunnel) -> Self {
        Self {
            url: format!("http://127.0.0.1:{local_port}"),
            local_port,
            tunnel: Some(tunnel),
        }
    }

    /// True while a tunnel carries the port, so the URL works only as long
    /// as this `Forward` lives.
    pub fn is_tunnel(&self) -> bool {
        self.tunnel.is_some()
    }
}

/// Forwards `port` on the machine behind `connection` to this machine. Uses
/// the same port number when it's free here. This machine and WSL distros
/// (WSL forwards `localhost` itself) need no tunnel. ssh and kubectl forward
/// with their own tools. Other prefixes, such as `docker exec -i`, relay each
/// connection through `ppm connect` on the machine.
pub async fn forward(connection: &Connection, port: u16) -> anyhow::Result<Forward> {
    let (prefix, ppm_path) = match connection {
        Connection::Sidecar { .. } => return Ok(Forward::direct(port)),
        Connection::Command { argv_prefix, .. } if remote::program(argv_prefix) == "wsl" => {
            return Ok(Forward::direct(port))
        }
        Connection::Command {
            argv_prefix,
            ppm_path,
        } => (argv_prefix, ppm_path),
        Connection::Http { url, .. } => {
            bail!(
                "Can't forward ports from {url}. Forward port {port} through your tunnel or proxy."
            )
        }
    };
    let local_port = free_port(port)?;
    match tunnel(prefix, local_port, port)? {
        Some(argv) => spawn_tunnel(&argv, local_port, port).await,
        None => relay(prefix, ppm_path, local_port, port),
    }
}

async fn spawn_tunnel(argv: &[String], local_port: u16, port: u16) -> anyhow::Result<Forward> {
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
            return Ok(Forward::tunneled(
                local_port,
                Tunnel::Process { _child: child },
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("Forwarding port {port} took longer than 10 s.")
}

/// Listens on `local_port` and pipes each connection through
/// `<prefix> <ppm_path> connect <port>`.
fn relay(prefix: &[String], ppm_path: &str, local_port: u16, port: u16) -> anyhow::Result<Forward> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, local_port))?;
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let (prefix, ppm_path, port) = (prefix.to_vec(), ppm_path.to_string(), port.to_string());
    let accept = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let os = remote::os_of(&ppm_path);
            let mut command = remote::command(&prefix, os, &[&ppm_path, "connect", &port]);
            command.stderr(Stdio::null());
            let Ok(mut child) = command.spawn() else {
                continue;
            };
            let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take())
            else {
                continue;
            };
            tokio::spawn(async move {
                let (mut read, mut write) = socket.into_split();
                let up = async move {
                    let _ = tokio::io::copy(&mut read, &mut stdin).await;
                };
                let down = async move {
                    let _ = tokio::io::copy(&mut stdout, &mut write).await;
                    let _ = write.shutdown().await;
                };
                tokio::join!(up, down);
                let _ = child.wait().await;
            });
        }
    });
    Ok(Forward::tunneled(local_port, Tunnel::Relay { accept }))
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

/// The command that forwards `remote` to `local` for a connection prefix, or
/// `None` when the prefix has no forwarding of its own.
fn tunnel(prefix: &[String], local: u16, remote: u16) -> anyhow::Result<Option<Vec<String>>> {
    match remote::program(prefix).as_str() {
        "ssh" => {
            let spec = format!("127.0.0.1:{local}:localhost:{remote}");
            let mut options = remote::SSH_KEEPALIVE.to_vec();
            options.extend(["-N", "-o", "ExitOnForwardFailure=yes", "-L", &spec]);
            Ok(Some(remote::ssh_with(prefix, &options)))
        }
        "kubectl" => kubectl_port_forward(prefix, local, remote).map(Some),
        _ => Ok(None),
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
            Some(strings(&[
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
            ]))
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
            Some(strings(&[
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
            ]))
        );
        assert_eq!(
            tunnel(
                &strings(&["kubectl", "exec", "-i", "deploy/web", "--"]),
                3000,
                3000
            )
            .unwrap(),
            Some(strings(&[
                "kubectl",
                "port-forward",
                "--address",
                "127.0.0.1",
                "deploy/web",
                "3000:3000"
            ]))
        );
    }

    #[test]
    fn other_prefixes_relay_through_ppm() {
        assert_eq!(
            tunnel(&strings(&["docker", "exec", "-i", "box"]), 3000, 3000).unwrap(),
            None
        );
    }
}
