//! Connections against stand-ins for `everyport stdio`, `ssh` and `everyport serve`.
#![cfg(unix)]

use everyport::client::{connect, install, Connection, Update};
use everyport::protocol::{Call, Event, Hello, HostInfo, Os, ProcRef, Snapshot, PROTOCOL_VERSION};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;

fn fixture() -> Snapshot {
    serde_json::from_str(include_str!(
        "../../../packages/protocol/fixtures/snapshot.json"
    ))
    .unwrap()
}

fn hello() -> Event {
    Event::Hello(Hello {
        protocol: PROTOCOL_VERSION,
        everyport_version: "0.1.0".into(),
        host: HostInfo {
            hostname: "box".into(),
            os: Os::Linux,
            arch: "aarch64".into(),
            cores: 8,
        },
    })
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("everyport-connect-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// This release's `everyport`: it prints a banner, `hello` and the fixture snapshot, answers
/// two requests (refresh succeeds, anything else fails), then exits with an
/// error on stderr.
fn fake_everyport(path: &Path) {
    let hello = serde_json::to_string(&hello()).unwrap();
    let snapshot = serde_json::to_string(&Event::Snapshot(fixture())).unwrap();
    let version = install::VERSION;
    write_script(
        path,
        &format!(
            r#"[ "$1" = --version ] && {{ echo "everyport {version}"; exit 0; }}
echo 'Welcome to box'
cat <<'EOF'
{hello}
{snapshot}
EOF
for n in 1 2; do
  IFS= read -r line || exit 0
  id=$(printf '%s' "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
  case "$line" in
    *refresh*) echo "{{\"type\":\"result\",\"id\":$id,\"error\":null}}" ;;
    *) echo "{{\"type\":\"result\",\"id\":$id,\"error\":\"not running\"}}" ;;
  esac
done
echo 'everyport: lost the machine' >&2
exit 3
"#
        ),
    );
}

async fn next(updates: &mut UnboundedReceiver<Update>) -> Update {
    tokio::time::timeout(Duration::from_secs(10), updates.recv())
        .await
        .expect("an update within 10 s")
        .expect("the connection is still running")
}

fn stop() -> Call {
    Call::Stop {
        port: 3000,
        root: ProcRef {
            pid: 42,
            started_at: 1,
        },
        force: false,
        confirm_protected: false,
    }
}

#[tokio::test]
async fn sidecar_streams_events_answers_calls_and_reconnects() {
    let dir = temp_dir("sidecar");
    let everyport = dir.join("everyport");
    fake_everyport(&everyport);
    let (client, mut updates) = connect(Connection::Sidecar { path: everyport });

    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(next(&mut updates).await, Update::Event(hello()));
    assert_eq!(
        next(&mut updates).await,
        Update::Event(Event::Snapshot(fixture()))
    );
    assert_eq!(client.call(Call::Refresh).await, Ok(()));
    assert_eq!(client.call(stop()).await, Err("not running".into()));

    assert_eq!(
        next(&mut updates).await,
        Update::Disconnected {
            error: "everyport: lost the machine".into(),
            retry_in: Some(Duration::from_secs(1))
        }
    );
    assert_eq!(
        client.call(Call::Refresh).await,
        Err("Not connected.".into())
    );
    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(next(&mut updates).await, Update::Event(hello()));

    drop(client);
    let _ = next(&mut updates).await; // the snapshot already on its way
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), updates.recv())
            .await
            .unwrap(),
        None
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_missing_program_is_reported_and_retried_with_backoff() {
    let (_client, mut updates) = connect(Connection::Command {
        argv_prefix: vec!["/nonexistent/ssh".into(), "devbox".into()],
        everyport_path: "everyport".into(),
    });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { error, retry_in } = next(&mut updates).await else {
        panic!()
    };
    assert!(
        error.starts_with("Couldn't run /nonexistent/ssh"),
        "{error}"
    );
    assert_eq!(retry_in, Some(Duration::from_secs(1)));
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { retry_in, .. } = next(&mut updates).await else {
        panic!()
    };
    assert_eq!(retry_in, Some(Duration::from_secs(2)));
}

#[tokio::test]
async fn a_sidecar_that_cant_run_keeps_retrying() {
    let dir = temp_dir("sidecar-denied");
    let sidecar = dir.join("everyport");
    // Not executable, like a quarantined app: `Permission denied (os error 13)`.
    std::fs::write(&sidecar, "").unwrap();
    let (_client, mut updates) = connect(Connection::Sidecar { path: sidecar });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { error, retry_in } = next(&mut updates).await else {
        panic!()
    };
    assert!(error.contains("Permission denied"), "{error}");
    assert_eq!(retry_in, Some(Duration::from_secs(1)));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_rejected_key_is_not_retried() {
    let dir = temp_dir("rejected");
    let ssh = dir.join("ssh");
    write_script(
        &ssh,
        "echo 'me@devbox: Permission denied (publickey).' >&2\nexit 255\n",
    );
    let (_client, mut updates) = connect(Connection::Command {
        argv_prefix: vec![ssh.to_string_lossy().into_owned(), "devbox".into()],
        everyport_path: "everyport".into(),
    });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(
        next(&mut updates).await,
        Update::Disconnected {
            error: "me@devbox: Permission denied (publickey).".into(),
            retry_in: None
        }
    );
    // The connection stops rather than trying again.
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), updates.recv())
            .await
            .unwrap(),
        None
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The release target for the machine running the tests, from the release
/// file names.
fn host_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        other => panic!("no release target for {other:?}"),
    }
}

#[tokio::test]
async fn probes_installs_and_connects_through_an_ssh_style_prefix() {
    let dir = temp_dir("ssh");
    // The remote home has a space, so every path must survive the remote shell.
    let home = dir.join("remote home");
    std::fs::create_dir_all(&home).unwrap();
    // Like ssh: skip `-o` options and the host, join the rest with spaces,
    // and run it in a shell.
    let ssh = dir.join("ssh");
    write_script(
        &ssh,
        &format!(
            "while [ \"$1\" = -o ]; do shift 2; done\nshift\nHOME='{}' PATH=/usr/bin:/bin:/usr/sbin:/sbin exec sh -c \"$*\"\n",
            home.display()
        ),
    );
    let prefix = vec![ssh.to_string_lossy().into_owned(), "box".into()];

    let probe = install::probe(&prefix).await.unwrap();
    let everyport_path = format!("{}/.local/bin/everyport", home.display());
    assert_eq!(probe.target, host_target());
    assert_eq!(probe.install_path, everyport_path);
    assert_eq!(probe.everyport_path, everyport_path);
    assert_eq!(probe.installed, None);
    assert!(!probe.up_to_date());

    let builds = dir.join("builds");
    std::fs::create_dir_all(&builds).unwrap();
    fake_everyport(&builds.join(format!("everyport-{}", host_target())));
    std::env::set_var("EVERYPORT_BINARY_DIR", &builds);

    std::fs::write(
        builds.join("SHA256SUMS"),
        format!("{}  everyport-{}\n", "0".repeat(64), host_target()),
    )
    .unwrap();
    let error = install::install(&prefix, &probe)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("doesn't match its SHA-256"), "{error}");
    assert!(!Path::new(&everyport_path).exists());
    std::fs::remove_file(builds.join("SHA256SUMS")).unwrap();

    install::install(&prefix, &probe).await.unwrap();
    let installed = std::fs::metadata(&everyport_path).unwrap();
    assert_eq!(installed.permissions().mode() & 0o777, 0o755);
    let probe = install::probe(&prefix).await.unwrap();
    assert_eq!(probe.installed.as_deref(), Some(install::VERSION));
    assert!(probe.up_to_date());

    let (_client, mut updates) = connect(Connection::Command {
        argv_prefix: prefix,
        everyport_path: probe.everyport_path,
    });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(next(&mut updates).await, Update::Event(hello()));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// An `everyport serve` stand-in on a random loopback port that accepts `token`.
async fn fake_serve(token: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut stream = BufReader::new(stream);
                let mut head = Vec::new();
                loop {
                    let mut line = String::new();
                    stream.read_line(&mut line).await.unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    head.push(line.to_ascii_lowercase());
                }
                let header = |name: &str| {
                    head.iter()
                        .find_map(|h| h.strip_prefix(name))
                        .map(|v| v.trim().to_string())
                };
                if header("authorization:") != Some(format!("bearer {token}")) {
                    let _ = stream
                        .get_mut()
                        .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n")
                        .await;
                    return;
                }
                if head[0].starts_with("get /events") {
                    let mut stream = stream.into_inner();
                    let events = format!(
                        ": ping\n\ndata: {}\n\ndata: {}\r\n\r\n",
                        serde_json::to_string(&hello()).unwrap(),
                        serde_json::to_string(&Event::Snapshot(fixture())).unwrap()
                    );
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n")
                        .await
                        .unwrap();
                    // Split mid-line, as TCP may.
                    let (a, b) = events.split_at(events.len() / 2);
                    stream.write_all(a.as_bytes()).await.unwrap();
                    stream.flush().await.unwrap();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    stream.write_all(b.as_bytes()).await.unwrap();
                    tokio::time::sleep(Duration::from_secs(60)).await;
                } else {
                    let length: usize = header("content-length:").unwrap().parse().unwrap();
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let mut stream = stream.into_inner();
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let error = if request["method"] == "refresh" {
                        "null"
                    } else {
                        "\"not running\""
                    };
                    let result = format!(
                        r#"{{"type":"result","id":{},"error":{error}}}"#,
                        request["id"]
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{result}",
                        result.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            });
        }
    });
    url
}

fn code(url: &str, token: &str) -> String {
    use base64::Engine as _;
    let json = serde_json::json!({ "url": url, "token": token }).to_string();
    format!(
        "everyport://{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    )
}

#[tokio::test]
async fn serve_code_streams_events_and_answers_calls() {
    let url = fake_serve("t0k").await;
    let (client, mut updates) = connect(Connection::from_code(&code(&url, "t0k")).unwrap());
    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(next(&mut updates).await, Update::Event(hello()));
    assert_eq!(
        next(&mut updates).await,
        Update::Event(Event::Snapshot(fixture()))
    );
    assert_eq!(client.call(Call::Refresh).await, Ok(()));
    assert_eq!(client.call(stop()).await, Err("not running".into()));
}

#[tokio::test]
async fn a_wrong_serve_token_says_so() {
    let url = fake_serve("t0k").await;
    let (_client, mut updates) = connect(Connection::from_code(&code(&url, "stale")).unwrap());
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { error, .. } = next(&mut updates).await else {
        panic!()
    };
    assert!(error.contains("rejected the connection code"), "{error}");
}
