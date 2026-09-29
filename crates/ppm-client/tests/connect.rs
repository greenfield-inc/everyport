//! Connections against stand-ins for `ppm stdio`, `ssh` and `ppm serve`.
#![cfg(unix)]

use ppm_client::protocol::{Call, Event, Hello, HostInfo, Os, ProcRef, Snapshot, PROTOCOL_VERSION};
use ppm_client::{connect, install, Connection, Update};
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
        ppm_version: "0.1.0".into(),
        host: HostInfo {
            hostname: "box".into(),
            os: Os::Linux,
            arch: "aarch64".into(),
            cores: 8,
        },
    })
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ppm-client-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A `ppm` that prints a banner, `hello` and the fixture snapshot, answers
/// two requests (refresh succeeds, anything else fails), then exits with an
/// error on stderr.
fn fake_ppm(path: &Path) {
    let hello = serde_json::to_string(&hello()).unwrap();
    let snapshot = serde_json::to_string(&Event::Snapshot(fixture())).unwrap();
    write_script(
        path,
        &format!(
            r#"[ "$1" = --version ] && {{ echo "ppm 0.1.0"; exit 0; }}
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
echo 'ppm: lost the machine' >&2
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
    }
}

#[tokio::test]
async fn sidecar_streams_events_answers_calls_and_reconnects() {
    let dir = temp_dir("sidecar");
    let ppm = dir.join("ppm");
    fake_ppm(&ppm);
    let (client, mut updates) = connect(Connection::Sidecar { path: ppm });

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
            error: "ppm: lost the machine".into(),
            retry_in: Duration::from_secs(1)
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
        ppm_path: "ppm".into(),
    });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { error, retry_in } = next(&mut updates).await else {
        panic!()
    };
    assert!(
        error.starts_with("Couldn't run /nonexistent/ssh"),
        "{error}"
    );
    assert_eq!(retry_in, Duration::from_secs(1));
    assert_eq!(next(&mut updates).await, Update::Connecting);
    let Update::Disconnected { retry_in, .. } = next(&mut updates).await else {
        panic!()
    };
    assert_eq!(retry_in, Duration::from_secs(2));
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
    // Like ssh: drop the host, join the rest with spaces, run it in a shell.
    let ssh = dir.join("ssh");
    write_script(
        &ssh,
        &format!(
            "shift\nHOME='{}' PATH=/usr/bin:/bin:/usr/sbin:/sbin exec sh -c \"$*\"\n",
            home.display()
        ),
    );
    let prefix = vec![ssh.to_string_lossy().into_owned(), "box".into()];

    let probe = install::probe(&prefix).await.unwrap();
    let ppm_path = format!("{}/.local/bin/ppm", home.display());
    assert_eq!(probe.target, host_target());
    assert_eq!(probe.install_path, ppm_path);
    assert_eq!(probe.ppm_path, ppm_path);
    assert_eq!(probe.installed, None);
    assert!(!probe.up_to_date());

    let builds = dir.join("builds");
    std::fs::create_dir_all(&builds).unwrap();
    fake_ppm(&builds.join(format!("ppm-{}", host_target())));
    std::env::set_var("PPM_BINARY_DIR", &builds);

    std::fs::write(
        builds.join("SHA256SUMS"),
        format!("{}  ppm-{}\n", "0".repeat(64), host_target()),
    )
    .unwrap();
    let error = install::install(&prefix, &probe)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("doesn't match its SHA-256"), "{error}");
    assert!(!Path::new(&ppm_path).exists());
    std::fs::remove_file(builds.join("SHA256SUMS")).unwrap();

    install::install(&prefix, &probe).await.unwrap();
    let installed = std::fs::metadata(&ppm_path).unwrap();
    assert_eq!(installed.permissions().mode() & 0o777, 0o755);
    let probe = install::probe(&prefix).await.unwrap();
    assert_eq!(probe.installed.as_deref(), Some(install::VERSION));
    assert!(probe.up_to_date());

    let (_client, mut updates) = connect(Connection::Command {
        argv_prefix: prefix,
        ppm_path: probe.ppm_path,
    });
    assert_eq!(next(&mut updates).await, Update::Connecting);
    assert_eq!(next(&mut updates).await, Update::Event(hello()));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A `ppm serve` stand-in on a random loopback port that accepts `token`.
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
        "ppm://{}",
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
