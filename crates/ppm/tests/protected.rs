//! `ppm stop` against a real protected server: a copy of `nc` named
//! `redis-server`, listening on a port in 39000-39999.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn redis_server(port: u16) -> Server {
    let dir = std::env::temp_dir().join(format!("ppm-protected-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("redis-server");
    std::fs::copy("/usr/bin/nc", &exe).expect("nc is installed");
    // macOS kills a copied system binary until it is signed again.
    #[cfg(target_os = "macos")]
    assert!(Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&exe)
        .output()
        .unwrap()
        .status
        .success());
    let child = Command::new(&exe)
        .args(["-l", &port.to_string()])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server { child, dir };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !listed(port) {
        assert!(
            Instant::now() < deadline,
            "redis-server never listed :{port}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    server
}

fn ppm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ppm"))
        .args(args)
        .output()
        .unwrap()
}

fn listed(port: u16) -> bool {
    let list = String::from_utf8(ppm(&["list", "--json"]).stdout).unwrap();
    let list = list.replace(char::is_whitespace, "");
    list.contains(&format!("\"port\":{port},"))
}

#[test]
fn stop_refuses_a_protected_server_unless_told_it_is() {
    let port = 39_000 + (std::process::id() % 1000) as u16;
    let mut server = redis_server(port);

    let refused = ppm(&["stop", &port.to_string()]);
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        format!(
            "ppm: redis-server :{port} is protected. Run `ppm stop {port} --protected` to stop it anyway.\n"
        )
    );
    assert!(server.child.try_wait().unwrap().is_none());

    let stopped = ppm(&["stop", &port.to_string(), "--protected"]);
    assert!(stopped.status.success(), "{stopped:?}");
    assert!(server.child.try_wait().unwrap().is_some());
}
