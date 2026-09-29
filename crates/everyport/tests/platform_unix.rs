//! The macOS and Linux platforms, checked against processes these tests start.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use everyport::platform::{native, Listener, ProcInfo};
use everyport::protocol::ProcRef;
use std::io;
use std::net::TcpListener;
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
        let dir =
            std::env::temp_dir().join(format!("everyport-test-{}-{name}", std::process::id()));
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

/// Root's listener, as a normal user sees it. Starting it takes passwordless
/// sudo, which CI's runners have, so it fails there rather than skip. Elsewhere
/// the test says why it skips.
#[test]
fn another_users_listener_shows_port_and_owner() {
    use everyport::platform::OtherListener;
    let sudo = || {
        let mut command = Command::new("sudo");
        command.arg("-n");
        command
    };
    let can_sudo = sudo().arg("true").status().is_ok_and(|s| s.success());
    if unsafe { libc::geteuid() } == 0 || !can_sudo {
        let why = "needs a user other than root with passwordless sudo";
        assert!(std::env::var_os("CI").is_none(), "{why}");
        eprintln!("skipped: {why}");
        return;
    }
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let serve = format!(
        "import socket, time; s = socket.socket(); s.bind(('127.0.0.1', {port})); s.listen(); time.sleep(30)"
    );
    let mut root = sudo().args(["python3", "-c", &serve]).spawn().unwrap();
    // sudo runs as root, so only root can stop it.
    let kill = || sudo().args(["kill", &root.id().to_string()]).status();

    let shown = |l: &OtherListener| {
        l.port == port && l.address == "127.0.0.1" && l.owner.as_deref() == Some("root")
    };
    let mut found = Vec::new();
    for _ in 0..200 {
        // A new platform each time, so macOS doesn't serve a cached answer.
        found = native().other_listeners().unwrap();
        if found.iter().any(shown) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    let listed = native().listeners().unwrap();
    let listener = found.iter().find(|l| shown(l)).cloned();
    // Checked while the process runs.
    let from_sudo = listener
        .as_ref()
        .and_then(|l| l.pid)
        .map(|pid| descends_from(pid, root.id()));
    kill().unwrap();
    root.wait().unwrap();

    let listener = listener.unwrap_or_else(|| panic!("root on {port} not in {found:?}"));
    assert!(listed.iter().all(|l| l.port != port), "{listed:?}");
    // Linux can't tell a normal user which process holds the socket; macOS can.
    if cfg!(target_os = "macos") {
        assert_eq!(from_sudo, Some(true), "{listener:?} vs sudo {}", root.id());
        let name = listener.process_name.as_deref().unwrap_or_default();
        assert!(name.to_lowercase().starts_with("python"), "{listener:?}");
    } else {
        assert_eq!((listener.pid, listener.process_name), (None, None));
    }
}

/// Whether `ancestor` is `pid` or one of its parents, as `ps` shows them.
fn descends_from(mut pid: u32, ancestor: u32) -> bool {
    while pid > 1 {
        if pid == ancestor {
            return true;
        }
        let ps = Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let Ok(parent) = String::from_utf8_lossy(&ps.stdout).trim().parse() else {
            return false;
        };
        pid = parent;
    }
    false
}

#[test]
fn processes_report_name_parent_and_start_time() {
    // `/proc/<pid>/stat` wraps the name in parentheses.
    let copy = SleepCopy::new("we) ird");
    let spawned_at = everyport::now_ms();
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
    let copy = SleepCopy::new("everyport-details");
    let dir = copy.program.parent().unwrap();
    // An empty argv[0] must keep its place.
    let mut child = start(
        Command::new(&copy.program)
            .arg0("")
            .arg("30")
            .current_dir(dir)
            .env_clear()
            .env("EVERYPORT_TEST_SESSION", "abc=123")
            .env("EVERYPORT_TEST_OTHER", "x"),
    );

    let details = native().details(
        child.id(),
        &["EVERYPORT_TEST_SESSION", "EVERYPORT_TEST_MISSING"],
    );
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
        [("EVERYPORT_TEST_SESSION".to_string(), "abc=123".to_string())]
    );
    environment.sort();
    assert_eq!(
        environment,
        [
            ("EVERYPORT_TEST_OTHER".to_string(), "x".to_string()),
            ("EVERYPORT_TEST_SESSION".to_string(), "abc=123".to_string()),
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
