//! `everyport remote` and `everyport --on`, run as a user would, with a home folder of
//! their own. The machine `here` is this one, reached through `env`, which
//! passes its arguments on like `docker exec` does.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EVERYPORT: &str = env!("CARGO_BIN_EXE_everyport");

struct Home(PathBuf);

impl Home {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("everyport-{test}-{}.noindex", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn everyport(&self, args: &[&str]) -> Output {
        // `everyport` on PATH is this build, so `here` has it installed.
        let bin = Path::new(EVERYPORT).parent().unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        Command::new(EVERYPORT)
            .args(args)
            .env("HOME", &self.0)
            .env("PATH", path)
            .env("PANE_DIR", self.0.join(".pane"))
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn adds_lists_and_removes_machines() {
    let home = Home::new("remote");
    let added = home.everyport(&["remote", "add", "here", "--", "env"]);
    assert!(added.status.success(), "{}", stderr(&added));
    assert_eq!(
        stdout(&added),
        "Added here. Run `everyport --on here` to see its servers.\n"
    );
    // printf '{"url":"http://127.0.0.1:7767","token":"s3cret"}' | base64 | tr '+/' '-_' | tr -d '='
    let code = "everyport://eyJ1cmwiOiJodHRwOi8vMTI3LjAuMC4xOjc3NjciLCJ0b2tlbiI6InMzY3JldCJ9";
    let added = home.everyport(&["remote", "add", "mini", "--code", code]);
    assert!(added.status.success(), "{}", stderr(&added));
    let again = home.everyport(&["remote", "add", "mini", "--code", code]);
    assert!(stdout(&again).starts_with("Updated mini."));

    let list = home.everyport(&["remote", "list"]);
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        stdout(&list),
        format!(
            "NAME  CONNECTION             FROM   EVERYPORT\n\
             here  env                    saved  {version}\n\
             mini  http://127.0.0.1:7767  saved  everyport serve\n"
        )
    );
    assert!(!stdout(&list).contains("s3cret"));

    let removed = home.everyport(&["remote", "rm", "mini"]);
    assert_eq!(stdout(&removed), "Removed mini.\n");
    let missing = home.everyport(&["remote", "rm", "mini"]);
    assert!(!missing.status.success());
    assert_eq!(stderr(&missing), "everyport: no saved machine named mini\n");
}

#[test]
fn on_runs_the_command_on_the_named_machine() {
    let home = Home::new("on");
    home.everyport(&["remote", "add", "here", "--", "env"]);

    let list = home.everyport(&["--on", "here", "list", "--json"]);
    assert!(list.status.success(), "{}", stderr(&list));
    let snapshot: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(snapshot["servers"].is_array());

    let unknown = home.everyport(&["--on", "nowhere", "list"]);
    assert!(!unknown.status.success());
    assert_eq!(
        stderr(&unknown),
        "everyport: No machine named nowhere. Add it with `everyport remote add nowhere -- ssh nowhere`, or see `everyport remote list`.\n"
    );

    let local_only = home.everyport(&["--on", "here", "doctor"]);
    assert!(!local_only.status.success());
    assert_eq!(
        stderr(&local_only),
        "everyport: --on works with list, watch, stop, restart, open, clean and the terminal UI\n"
    );
}

/// A protocol fixture makes cross-user listeners reproducible without needing
/// privileges or relying on the processes that happen to run on the test host.
fn list_snapshot(snapshot: everyport::protocol::Snapshot, case: &str) -> String {
    use base64::Engine as _;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut events, _) = listener.accept().unwrap();
        events
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(&events);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("GET /events "));
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        write!(
            events,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        writeln!(events, "data: {{\"type\":\"hello\",\"protocol\":1,\"everyport_version\":\"0.1.0\",\"host\":{{\"hostname\":\"fixture\",\"os\":\"linux\",\"arch\":\"aarch64\",\"cores\":2}}}}\n").unwrap();
        let mut snapshot = snapshot;
        snapshot.taken_at = 1;
        writeln!(
            events,
            "data: {}\n",
            serde_json::to_string(&everyport::protocol::Event::Snapshot(snapshot.clone())).unwrap()
        )
        .unwrap();
        let (mut call, _) = listener.accept().unwrap();
        call.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(&call);
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                if key.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request["method"], "refresh");
        let reply =
            serde_json::json!({"type":"result", "id":request["id"], "error":null}).to_string();
        write!(
            call,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        snapshot.taken_at = 2;
        writeln!(
            events,
            "data: {}\n",
            serde_json::to_string(&everyport::protocol::Event::Snapshot(snapshot)).unwrap()
        )
        .unwrap();
    });
    let home = Home::new(case);
    let code = format!(
        "everyport://{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::json!({"url":url,"token":"fixture-token"}).to_string())
    );
    assert!(home
        .everyport(&["remote", "add", "fixture", "--code", &code])
        .status
        .success());
    let output = home.everyport(&["--on", "fixture", "list"]);
    server.join().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    stdout(&output)
}

#[test]
fn plain_list_includes_other_ports_and_only_calls_a_truly_empty_snapshot_empty() {
    use everyport::protocol::{OtherPort, Snapshot};
    let mut snapshot: Snapshot = serde_json::from_str(include_str!(
        "../../../packages/protocol/fixtures/snapshot.json"
    ))
    .unwrap();
    snapshot.other_ports = vec![
        OtherPort {
            port: 39301,
            addresses: vec!["127.0.0.1".into(), "::1".into()],
            owner: Some("postgres".into()),
            process_name: Some("postgresql".into()),
        },
        OtherPort {
            port: 39302,
            addresses: vec!["0.0.0.0".into()],
            owner: Some("root".into()),
            process_name: None,
        },
    ];
    let mixed = list_snapshot(snapshot.clone(), "list-mixed");
    assert!(mixed.find("Other ports").unwrap() > mixed.find("MEMORY").unwrap());
    snapshot.servers.clear();
    let only_other = list_snapshot(snapshot.clone(), "list-other");
    for output in [mixed, only_other] {
        assert!(output.contains("Other ports"));
        assert!(!output.contains("Nothing listening"));
        assert!(output
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>()
                == [":39301", "127.0.0.1,", "::1", "postgres", "postgresql"]));
        assert!(output.lines().any(
            |line| line.split_whitespace().collect::<Vec<_>>() == [":39302", "0.0.0.0", "root"]
        ));
    }
    snapshot.other_ports.clear();
    assert_eq!(
        list_snapshot(snapshot, "list-empty"),
        "Nothing listening on ports 3000-65535.\n"
    );
}
