//! The macOS and Linux platforms, checked against processes these tests start.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use ppm_core::platform::{native, Listener, ProcInfo};
use ppm_core::protocol::ProcRef;
use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

/// A copy of `sleep` named `name` in a fresh folder. macOS hides the
/// environment of Apple's own binaries, so tests don't run `/bin/sleep`.
fn sleep_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ppm-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let program = dir.canonicalize().unwrap().join(name);
    let sleep = Command::new("sh")
        .args(["-c", "command -v sleep"])
        .output()
        .unwrap();
    let sleep = String::from_utf8(sleep.stdout).unwrap();
    if !program.exists() {
        std::fs::copy(sleep.trim(), &program).unwrap();
    }
    program
}

fn sleeper() -> Child {
    Command::new("sleep").arg("30").spawn().unwrap()
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
fn processes_report_name_parent_and_start_time() {
    // `/proc/<pid>/stat` wraps the name in parentheses.
    let program = sleep_copy("we) ird");
    let spawned_at = ppm_core::now_ms();
    let mut child = Command::new(&program).arg("30").spawn().unwrap();

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
fn details_report_cwd_args_and_only_requested_env() {
    let program = sleep_copy("ppm-details");
    let dir = program.parent().unwrap();
    let mut child = Command::new(&program)
        .arg("30")
        .current_dir(dir)
        .env("PPM_TEST_SESSION", "abc=123")
        .env("PPM_TEST_OTHER", "x")
        .spawn()
        .unwrap();

    let details = native().details(child.id(), &["PPM_TEST_SESSION", "PPM_TEST_MISSING"]);
    child.kill().unwrap();
    child.wait().unwrap();

    let details = details.expect("own process is readable");
    assert_eq!(details.cwd.as_deref(), dir.to_str());
    assert_eq!(details.args, [program.to_str().unwrap(), "30"]);
    assert_eq!(
        details.env,
        [("PPM_TEST_SESSION".to_string(), "abc=123".to_string())]
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
    std::thread::sleep(Duration::from_millis(100));
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
