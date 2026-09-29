//! Stopping a console server on Windows through the engine. CI runs it on
//! windows-latest. It has its own `main` so it can host the Ctrl+C helper,
//! as `ppm` does.

fn main() {
    ppm_core::platform::run_helper();
    #[cfg(windows)]
    {
        windows::ctrl_c_stops_a_server_on_its_own_console();
        windows::a_console_shared_with_another_process_is_terminated();
        println!("windows_stop: 2 passed");
    }
}

#[cfg(windows)]
mod windows {
    use ppm_core::engine::Engine;
    use ppm_core::platform::native;
    use ppm_core::protocol::{Call, Config};
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// On SIGINT (Ctrl+C), waits 500 ms, writes `graceful` to its folder and
    /// exits. The file exists only if the stop left it that time.
    const SERVER: &str = "process.on('SIGINT',()=>setTimeout(()=>{require('fs').writeFileSync('graceful','1');process.exit(0)},500));require('http').createServer().listen(+process.argv[1])";

    pub fn ctrl_c_stops_a_server_on_its_own_console() {
        let graceful = stop(|port| format!("node -e \"{SERVER}\" {port}"));
        assert!(
            graceful,
            "the server was terminated, not stopped with Ctrl+C"
        );
    }

    /// `ping` shares the console but isn't part of the server, so it must
    /// not get a Ctrl+C.
    pub fn a_console_shared_with_another_process_is_terminated() {
        let graceful = stop(|port| {
            format!("start /b ping -n 30 127.0.0.1 >nul & node -e \"{SERVER}\" {port}")
        });
        assert!(!graceful, "sent Ctrl+C to a console shared with ping");
    }

    /// Runs `cmd /c <command>` in a new console, stops the server it starts,
    /// and returns whether the server exited through its Ctrl+C handler.
    fn stop(command: impl Fn(u16) -> String) -> bool {
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let dir = std::env::temp_dir().join(format!("ppm-stop-{port}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut shell = Command::new("cmd")
            .raw_arg(format!("/d /s /c \"{}\"", command(port)))
            .current_dir(&dir)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .expect("cmd");

        let mut engine = Engine::new(native(), Config::default());
        let deadline = Instant::now() + Duration::from_secs(30);
        let root = loop {
            let (snapshot, _) = engine.scan();
            if let Some(server) = snapshot.servers.iter().find(|s| s.port == port) {
                break server.root;
            }
            assert!(Instant::now() < deadline, "nothing listening on {port}");
            std::thread::sleep(Duration::from_millis(200));
        };
        engine
            .call(&Call::Stop {
                port,
                root,
                force: false,
                confirm_protected: false,
            })
            .unwrap();
        while engine.pending() || shell.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = shell.kill();
                panic!("the server or its cmd outlived the stop");
            }
            std::thread::sleep(Duration::from_millis(100));
            engine.scan();
        }
        let graceful = dir.join("graceful").exists();
        let _ = std::fs::remove_dir_all(&dir);
        graceful
    }
}
