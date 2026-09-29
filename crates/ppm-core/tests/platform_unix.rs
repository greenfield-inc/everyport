//! The macOS and Linux platforms, checked against processes these tests start.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use ppm_core::platform::{native, Listener, ProcInfo};
use ppm_core::protocol::ProcRef;
use std::io;
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::Duration;

/// A copy of `sleep` named `name` in its own folder, removed on drop. macOS
/// hides the environment of Apple's own binaries, so tests don't run
/// `/bin/sleep` when they read it.
struct SleepCopy {
    program: PathBuf,
}

impl SleepCopy {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ppm-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.canonicalize().unwrap().join(name);
        let sleep = Command::new("sh")
            .args(["-c", "command -v sleep"])
            .output()
            .unwrap();
        std::fs::copy(String::from_utf8(sleep.stdout).unwrap().trim(), &program).unwrap();
        Self { program }
    }
}

impl Drop for SleepCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.program.parent().unwrap());
    }
}

/// Spawns `command` and returns once it runs its own program. On Linux, a
/// freshly copied program can be busy while another test's fork still holds
/// it open for writing, and a child shows this process's arguments until its
/// exec finishes.
fn start(command: &mut Command) -> Child {
    let child = loop {
        match command.spawn() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy => {
                sleep(Duration::from_millis(10))
            }
            spawned => break spawned.unwrap(),
        }
    };
    let ours: Vec<String> = std::env::args().collect();
    for _ in 0..500 {
        match native().details(child.id(), &[]) {
            Some(d) if !d.args.is_empty() && d.args != ours => return child,
            _ => sleep(Duration::from_millis(10)),
        }
    }
    panic!("{command:?} did not start");
}

fn sleeper() -> Child {
    start(Command::new("sleep").arg("30"))
}

fn proc_info(pid: u32) -> ProcInfo {
    native()
        .processes()
        .unwrap()
        .into_iter()
        .find(|p| p.proc.pid == pid)
        .expect("process is listed")
}

fn process_cpu_ns() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) };
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

#[test]
fn listeners_include_sockets_we_bind() {
    let listeners: Vec<Listener> = ["127.0.0.1:0", "[::1]:0"]
        .into_iter()
        .map(|address| {
            let socket = TcpListener::bind(address).unwrap();
            let local = socket.local_addr().unwrap();
            std::mem::forget(socket);
            Listener {
                port: local.port(),
                pid: std::process::id(),
                address: local.ip().to_string(),
            }
        })
        .collect();
    let found = native().listeners().unwrap();
    for listener in &listeners {
        assert!(found.contains(listener), "{listener:?} not in {found:?}");
    }
    assert_eq!(listeners[1].address, "::1");
}

#[test]
fn connections_count_accepted_sockets_by_local_port() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let clients: Vec<TcpStream> = (0..2)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    let accepted: Vec<TcpStream> = (0..2).map(|_| server.accept().unwrap().0).collect();

    let counts = native().connections().expect("connections are known");
    drop((clients, accepted));

    assert_eq!(counts.get(&port), Some(&2));
}

#[test]
fn processes_report_name_parent_and_start_time() {
    // `/proc/<pid>/stat` wraps the name in parentheses.
    let copy = SleepCopy::new("we) ird");
    let spawned_at = ppm_core::now_ms();
    let mut child = start(Command::new(&copy.program).arg("30"));

    let info = proc_info(child.id());
    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(info.name, "we) ird");
    assert_eq!(info.parent, Some(std::process::id()));
    assert!(
        info.proc.started_at.abs_diff(spawned_at) < 2_000,
        "{info:?} vs {spawned_at}"
    );
}

#[test]
fn details_and_environment_match_the_launch() {
    let copy = SleepCopy::new("ppm-details");
    let dir = copy.program.parent().unwrap();
    // An empty argv[0] must keep its place.
    let mut child = start(
        Command::new(&copy.program)
            .arg0("")
            .arg("30")
            .current_dir(dir)
            .env_clear()
            .env("PPM_TEST_SESSION", "abc=123")
            .env("PPM_TEST_OTHER", "x"),
    );

    let details = native().details(child.id(), &["PPM_TEST_SESSION", "PPM_TEST_MISSING"]);
    let mut environment = native()
        .environment(child.id())
        .expect("own process is readable");
    child.kill().unwrap();
    child.wait().unwrap();

    let details = details.expect("own process is readable");
    assert_eq!(details.cwd.as_deref(), dir.to_str());
    assert_eq!(details.args, ["", "30"]);
    assert_eq!(
        details.env,
        [("PPM_TEST_SESSION".to_string(), "abc=123".to_string())]
    );
    environment.sort();
    assert_eq!(
        environment,
        [
            ("PPM_TEST_OTHER".to_string(), "x".to_string()),
            ("PPM_TEST_SESSION".to_string(), "abc=123".to_string()),
        ]
    );
}

#[test]
fn usage_tracks_cpu_time_and_touched_memory() {
    let platform = native();
    let pid = std::process::id();
    let before = platform.usage(pid).unwrap();
    let cpu_start = process_cpu_ns();
    let mut spin = 0u64;
    while process_cpu_ns() - cpu_start < 200_000_000 {
        spin = std::hint::black_box(spin.wrapping_add(1));
    }
    let block = std::hint::black_box(vec![1u8; 64 << 20]);
    let after = platform.usage(pid).unwrap();
    let cpu_elapsed = process_cpu_ns() - cpu_start;
    drop(block);

    let cpu = after.cpu_time_ns - before.cpu_time_ns;
    // Other tests run on threads of this process too, so allow some slack.
    assert!(cpu >= 190_000_000, "{cpu} ns");
    assert!(
        cpu <= cpu_elapsed + 50_000_000,
        "{cpu} ns of {cpu_elapsed} ns"
    );
    assert!(
        after.memory >= before.memory + (60 << 20),
        "{before:?} -> {after:?}"
    );
}

#[test]
fn signal_checks_the_start_time_first() {
    let platform = native();
    let mut child = sleeper();
    let seen = proc_info(child.id()).proc;

    let stale = ProcRef {
        started_at: seen.started_at - 1_000,
        ..seen
    };
    assert!(platform.signal(stale, false).is_err());
    sleep(Duration::from_millis(100));
    assert!(
        child.try_wait().unwrap().is_none(),
        "stale ProcRef was signalled"
    );

    platform.signal(seen, false).unwrap();
    assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));

    let mut child = sleeper();
    platform.signal(proc_info(child.id()).proc, true).unwrap();
    assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
}
