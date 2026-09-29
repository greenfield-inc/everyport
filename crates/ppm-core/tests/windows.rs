//! The Windows platform against a real server. CI runs it on windows-latest.
#![cfg(windows)]

use ppm_core::platform::{native, Listener, Platform};
use ppm_core::protocol::ProcRef;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server() -> (Server, u16) {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new("python")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PPM_TEST_MARKER", "found-me")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("python on PATH");
    (Server(child), port)
}

fn wait_for_listener(platform: &dyn Platform, port: u16) -> Listener {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let found = platform
            .listeners()
            .unwrap()
            .into_iter()
            .find(|l| l.port == port);
        if let Some(listener) = found {
            return listener;
        }
        assert!(Instant::now() < deadline, "nothing listening on {port}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn started_at(platform: &dyn Platform, pid: u32) -> u64 {
    let procs = platform.processes().unwrap();
    procs
        .iter()
        .find(|p| p.proc.pid == pid)
        .expect("in the process table")
        .proc
        .started_at
}

#[test]
fn finds_a_python_server_with_its_details_and_memory() {
    let platform = native();
    let (server, port) = start_server();
    let listener = wait_for_listener(&*platform, port);
    assert_eq!(listener.pid, server.0.id());
    assert_eq!(listener.address, "127.0.0.1");

    let procs = platform.processes().unwrap();
    let info = procs
        .iter()
        .find(|p| p.proc.pid == listener.pid)
        .expect("in the process table");
    assert_eq!(info.name.to_lowercase(), "python.exe");
    assert_eq!(info.parent, Some(std::process::id()));
    let age = ppm_core::now_ms() - info.proc.started_at;
    assert!(age < 60_000, "started {age} ms ago");

    // Windows variable names ignore case.
    let details = platform
        .details(listener.pid, &["ppm_test_marker"])
        .expect("details");
    assert_eq!(details.cwd.as_deref(), Some(env!("CARGO_MANIFEST_DIR")));
    assert!(
        details.args.iter().any(|a| a == "http.server"),
        "{:?}",
        details.args
    );
    assert_eq!(
        details.env,
        [("ppm_test_marker".to_string(), "found-me".to_string())]
    );

    let usage = platform.usage(listener.pid).expect("usage");
    assert!(
        (1 << 20..500 << 20).contains(&usage.memory),
        "{} bytes",
        usage.memory
    );
    let host = platform.memory();
    assert!(
        usage.memory < host.used && host.used < host.total,
        "{host:?}"
    );
}

#[test]
fn stop_checks_the_start_time_before_terminating() {
    let platform = native();
    let (mut server, port) = start_server();
    let pid = wait_for_listener(&*platform, port).pid;
    let started_at = started_at(&*platform, pid);

    let reused = ProcRef {
        pid,
        started_at: started_at - 1,
    };
    assert!(platform.signal(reused, false).is_err());
    assert!(
        server.0.try_wait().unwrap().is_none(),
        "stopped with a stale start time"
    );

    platform.signal(ProcRef { pid, started_at }, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "still running after stop");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Every Windows install runs the RPC endpoint mapper on port 135, in an
/// svchost that runs as NETWORK SERVICE.
#[test]
fn a_service_port_is_another_users_with_its_account() {
    let platform = native();
    assert!(platform.listeners().unwrap().iter().all(|l| l.port != 135));
    let rpc = platform
        .other_listeners()
        .unwrap()
        .into_iter()
        .find(|l| l.port == 135)
        .expect("port 135 listens");
    assert_eq!(rpc.owner.as_deref(), Some("NETWORK SERVICE"));
    let name = platform
        .processes()
        .unwrap()
        .into_iter()
        .find(|p| Some(p.proc.pid) == rpc.pid)
        .map(|p| p.name);
    assert_eq!(name.as_deref(), Some("svchost.exe"));
}
