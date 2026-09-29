//! Engine behavior against a fake `Platform`. Expected values follow
//! WhatThePort's rules and the defaults in `Config`.

use ppm_core::engine::Engine;
use ppm_core::platform::{
    Listener, MemoryStats, OtherListener, Platform, ProcDetails, ProcInfo, ProcUsage,
};
use ppm_core::protocol::{
    AlertKind, Call, CleanUpReason, Config, OtherPort, ProcRef, Server, ServerStatus, Snapshot,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};

const MB: u64 = 1024 * 1024;
const T0: u64 = 1_790_000_000_000;
const HOUR: u64 = 3600 * 1000;

#[derive(Default)]
struct World {
    now: u64,
    procs: Vec<Proc>,
    listeners: Vec<Listener>,
    others: Vec<OtherListener>,
    /// None for a platform that can't count them.
    connections: Option<HashMap<u16, u32>>,
    used_memory: u64,
    /// Processes that ignore a terminate.
    stubborn: HashSet<u32>,
    signals: Vec<(u32, bool)>,
    /// Processes the platform interrupts, as a Windows console does.
    interrupts: HashSet<u32>,
    interrupted: Vec<Vec<u32>>,
}

struct Proc {
    info: ProcInfo,
    details: ProcDetails,
    usage: ProcUsage,
    env: Vec<(String, String)>,
}

#[derive(Clone)]
struct Fake(Arc<Mutex<World>>);

impl Fake {
    fn new() -> Self {
        Fake(Arc::new(Mutex::new(World {
            now: T0,
            connections: Some(HashMap::new()),
            ..World::default()
        })))
    }

    fn world(&self) -> std::sync::MutexGuard<'_, World> {
        self.0.lock().unwrap()
    }

    /// Adds a process started `age` ms before now.
    fn run(&self, pid: u32, parent: u32, name: &str, args: &[&str], cwd: &str, age: u64) {
        let mut world = self.world();
        let started_at = world.now - age;
        world.procs.push(Proc {
            info: ProcInfo {
                proc: ProcRef { pid, started_at },
                parent: Some(parent),
                name: name.into(),
            },
            details: ProcDetails {
                cwd: Some(cwd.into()),
                args: args.iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
            },
            usage: ProcUsage::default(),
            env: Vec::new(),
        });
    }

    fn listen(&self, port: u16, pid: u32, address: &str) {
        self.world().listeners.push(Listener {
            port,
            pid,
            address: address.into(),
        });
    }

    fn set_usage(&self, pid: u32, memory: u64, cpu_time_ns: u64) {
        let mut world = self.world();
        let p = world.procs.iter_mut().find(|p| p.info.proc.pid == pid);
        p.unwrap().usage = ProcUsage {
            memory,
            cpu_time_ns,
        };
    }

    fn advance(&self, ms: u64) {
        self.world().now += ms;
    }

    fn proc_ref(&self, pid: u32) -> ProcRef {
        let world = self.world();
        world
            .procs
            .iter()
            .find(|p| p.info.proc.pid == pid)
            .unwrap()
            .info
            .proc
    }

    fn engine(&self) -> Engine {
        Engine::new(Box::new(self.clone()), Config::default())
    }
}

impl Platform for Fake {
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        let world = self.world();
        let alive = |pid| world.procs.iter().any(|p| p.info.proc.pid == pid);
        Ok(world
            .listeners
            .iter()
            .filter(|l| alive(l.pid))
            .cloned()
            .collect())
    }
    fn other_listeners(&self) -> io::Result<Vec<OtherListener>> {
        Ok(self.world().others.clone())
    }
    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        Ok(self.world().procs.iter().map(|p| p.info.clone()).collect())
    }
    fn details(&self, pid: u32, _env_keys: &[&str]) -> Option<ProcDetails> {
        let world = self.world();
        let p = world.procs.iter().find(|p| p.info.proc.pid == pid)?;
        Some(p.details.clone())
    }
    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        let world = self.world();
        world
            .procs
            .iter()
            .find(|p| p.info.proc.pid == pid)
            .map(|p| p.usage)
    }
    fn memory(&self) -> MemoryStats {
        MemoryStats {
            total: 16 * 1024 * MB,
            used: self.world().used_memory,
        }
    }
    fn cpu_percent(&self) -> f32 {
        0.0
    }
    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()> {
        let mut world = self.world();
        let Some(i) = world.procs.iter().position(|p| p.info.proc == target) else {
            return Err(io::ErrorKind::NotFound.into());
        };
        world.signals.push((target.pid, force));
        if force || !world.stubborn.contains(&target.pid) {
            world.procs.remove(i);
        }
        Ok(())
    }
    fn interrupt(&self, tree: &[ProcRef]) -> Vec<ProcRef> {
        let mut world = self.world();
        let reached: Vec<ProcRef> = tree
            .iter()
            .filter(|t| world.interrupts.contains(&t.pid))
            .copied()
            .collect();
        world
            .interrupted
            .push(reached.iter().map(|t| t.pid).collect());
        reached
    }
    fn connections(&self) -> Option<HashMap<u16, u32>> {
        self.world().connections.clone()
    }
    fn environment(&self, pid: u32) -> Option<Vec<(String, String)>> {
        let world = self.world();
        let p = world.procs.iter().find(|p| p.info.proc.pid == pid)?;
        Some(p.env.clone())
    }
    fn now_ms(&self) -> u64 {
        self.world().now
    }
}

fn project_dir() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

/// `zsh → npm run dev → sh -c "next dev" → next-server → turbopack`, as npm
/// leaves it on macOS: npm and Next.js overwrite their argv with a title.
fn next_dev(fake: &Fake) {
    let dir = project_dir();
    fake.run(100, 1, "zsh", &["-zsh"], dir, 5 * HOUR);
    fake.run(200, 100, "node", &["npm run dev"], dir, HOUR);
    fake.run(210, 200, "sh", &["sh", "-c", "next dev -p 3000"], dir, HOUR);
    fake.run(220, 210, "node", &["next-server (v15.1.0)"], dir, HOUR);
    let turbopack = ["/usr/local/bin/node", "/app/node_modules/.bin/turbopack"];
    fake.run(230, 220, "node", &turbopack, dir, HOUR);
    fake.listen(3000, 220, "127.0.0.1");
    fake.listen(3000, 220, "::1");
}

fn only_server(snapshot: &Snapshot) -> &Server {
    assert_eq!(snapshot.servers.len(), 1, "{:#?}", snapshot.servers);
    &snapshot.servers[0]
}

#[test]
fn builds_the_tree_from_the_command_the_user_ran() {
    let fake = Fake::new();
    next_dev(&fake);
    for (pid, memory) in [(200, 60), (210, 1), (220, 1000), (230, 170)] {
        fake.set_usage(pid, memory * MB, 0);
    }
    fake.world().used_memory = 10 * 1024 * MB;

    let (snapshot, _) = fake.engine().scan();
    let server = only_server(&snapshot);

    assert_eq!(server.port, 3000);
    assert_eq!(server.pid, 220);
    assert_eq!(server.root, fake.proc_ref(200));
    assert_eq!(server.process_name, "node");
    assert_eq!(server.addresses, ["127.0.0.1", "::1"]);
    assert_eq!(server.command.as_deref(), Some("npm run dev"));
    let tree: Vec<(&str, u32)> = server
        .processes
        .iter()
        .map(|p| (p.name.as_str(), p.depth))
        .collect();
    assert_eq!(
        tree,
        [
            ("npm run dev", 0),
            ("sh -c next dev -p 3000", 1),
            ("next-server", 2),
            ("turbopack", 3)
        ]
    );
    assert_eq!(server.memory, 1231 * MB);
    assert_eq!(
        snapshot.system.memory_other_apps,
        10 * 1024 * MB - 1231 * MB
    );
    assert_eq!(server.status, ServerStatus::Running);
    assert_eq!(server.clean_up, None);
}

#[test]
fn other_users_ports_show_once_each_with_owner_and_process_name() {
    let fake = Fake::new();
    next_dev(&fake);
    fake.run(900, 1, "docker-proxy", &[], "/", HOUR);
    let other = |port, address: &str, owner: &str, pid| OtherListener {
        port,
        address: address.into(),
        owner: Some(owner.into()),
        pid,
    };
    fake.world().others = vec![
        other(5432, "0.0.0.0", "root", Some(900)),
        other(5432, "::", "root", Some(900)),
        other(4000, "127.0.0.1", "postgres", None),
        // Below the default range, and a port already shown as a server.
        other(631, "127.0.0.1", "root", None),
        other(3000, "0.0.0.0", "root", None),
    ];

    let (snapshot, _) = fake.engine().scan();

    assert_eq!(
        snapshot.other_ports,
        [
            OtherPort {
                port: 4000,
                addresses: vec!["127.0.0.1".into()],
                owner: Some("postgres".into()),
                process_name: None,
            },
            OtherPort {
                port: 5432,
                addresses: vec!["0.0.0.0".into(), "::".into()],
                owner: Some("root".into()),
                process_name: Some("docker-proxy".into()),
            },
        ]
    );
    assert_eq!(only_server(&snapshot).port, 3000);
}

#[test]
fn stops_climbing_at_a_coding_agent() {
    let fake = Fake::new();
    let dir = project_dir();
    let claude = [
        "node",
        "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js",
    ];
    fake.run(50, 1, "node", &claude, dir, HOUR);
    fake.run(60, 50, "zsh", &["/bin/zsh", "-c", "pnpm dev"], dir, HOUR);
    fake.run(70, 60, "node", &["pnpm dev"], dir, HOUR);
    fake.run(
        80,
        70,
        "node",
        &["node", "/app/node_modules/vite/bin/vite.js"],
        dir,
        HOUR,
    );
    fake.listen(5173, 80, "127.0.0.1");

    let (snapshot, _) = fake.engine().scan();
    let server = only_server(&snapshot);

    assert_eq!(server.root.pid, 70);
    let names: Vec<&str> = server.processes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["pnpm dev", "vite"]);
}

#[test]
fn a_reused_parent_pid_is_not_an_ancestor() {
    let fake = Fake::new();
    let dir = project_dir();
    // pid 40 was npm's, but npm exited and a newer process took the pid.
    fake.run(40, 1, "node", &["npm run dev"], dir, 0);
    fake.run(41, 40, "node", &["node", "server.js"], dir, HOUR);
    fake.listen(8080, 41, "0.0.0.0");

    let (snapshot, _) = fake.engine().scan();

    assert_eq!(only_server(&snapshot).root.pid, 41);
}

#[test]
fn skips_ports_outside_the_range_app_helpers_and_system_daemons() {
    let fake = Fake::new();
    let dir = project_dir();
    fake.run(
        10,
        1,
        "python3",
        &["python3", "-m", "http.server", "80"],
        dir,
        HOUR,
    );
    fake.listen(80, 10, "0.0.0.0");
    let helper = ["/Applications/Figma.app/Contents/MacOS/figma_agent"];
    fake.run(11, 1, "figma_agent", &helper, dir, HOUR);
    fake.listen(44950, 11, "127.0.0.1");
    fake.run(12, 1, "rapportd", &["/usr/libexec/rapportd"], "/", HOUR);
    fake.listen(49152, 12, "0.0.0.0");
    // Homebrew and python.org Python run from inside Python.app.
    let python = "/opt/homebrew/Cellar/python@3.14/3.14.7/Frameworks/Python.framework/\
                  Versions/3.14/Resources/Python.app/Contents/MacOS/Python";
    fake.run(13, 1, "Python", &[python, "-m", "http.server"], dir, HOUR);
    fake.listen(8000, 13, "0.0.0.0");

    let (snapshot, _) = fake.engine().scan();

    assert_eq!(only_server(&snapshot).port, 8000);
}

#[test]
fn servers_started_from_the_filesystem_root_show_unless_the_os_ships_them() {
    let fake = Fake::new();
    // A container whose WORKDIR is `/`.
    fake.run(10, 1, "nc", &["nc", "-lk", "-p", "39010"], "/", HOUR);
    fake.listen(39010, 10, "0.0.0.0");
    fake.run(11, 1, "sshd", &["/usr/sbin/sshd", "-D"], "/", HOUR);
    fake.listen(39022, 11, "0.0.0.0");
    let resolved = ["/usr/lib/systemd/systemd-resolved"];
    fake.run(12, 1, "systemd-resolve", &resolved, "/", HOUR);
    fake.listen(39053, 12, "0.0.0.0");
    // nginx from a project folder is a dev server even though the OS ships it.
    fake.run(
        13,
        1,
        "nginx",
        &["/usr/sbin/nginx", "-c", "nginx.conf"],
        project_dir(),
        HOUR,
    );
    fake.listen(8080, 13, "127.0.0.1");

    let (snapshot, _) = fake.engine().scan();

    let ports: Vec<u16> = snapshot.servers.iter().map(|s| s.port).collect();
    assert_eq!(ports, [8080, 39010]);
}

#[test]
fn docker_proxy_shows_from_either_install_path() {
    let fake = Fake::new();
    for (pid, port, exe) in [
        (10, 39080, "/usr/bin/docker-proxy"),
        (11, 39081, "/usr/libexec/docker/docker-proxy"),
    ] {
        let args = [exe, "-proto", "tcp", "-host-port", &port.to_string()];
        fake.run(pid, 1, "docker-proxy", &args, "/", HOUR);
        fake.listen(port, pid, "0.0.0.0");
    }

    let (snapshot, _) = fake.engine().scan();

    let ports: Vec<u16> = snapshot.servers.iter().map(|s| s.port).collect();
    assert_eq!(ports, [39080, 39081]);
}

#[test]
fn cpu_is_cpu_time_over_wall_time_between_scans() {
    let fake = Fake::new();
    next_dev(&fake);
    let mut engine = fake.engine();
    engine.scan();

    fake.advance(2000);
    fake.set_usage(220, 0, 1_000_000_000);
    fake.set_usage(230, 0, 500_000_000);
    let (snapshot, _) = engine.scan();
    let server = only_server(&snapshot);

    // 1 s and 0.5 s of CPU over 2 s of wall time.
    assert_eq!(server.processes[2].cpu_percent, 50.0);
    assert_eq!(server.processes[3].cpu_percent, 25.0);
    assert_eq!(server.cpu_percent, 75.0);
}

#[test]
fn history_keeps_one_sample_per_10_seconds_for_10_minutes() {
    let fake = Fake::new();
    next_dev(&fake);
    let mut engine = fake.engine();
    let mut last = None;
    // Every 2 s for 11 minutes, with 1 s of CPU in the 2 s before 10:00.
    for second in (0..=660).step_by(2) {
        let cpu_time = if second >= 600 { 1_000_000_000 } else { 0 };
        fake.set_usage(220, 0, cpu_time);
        last = Some(engine.scan().0);
        fake.advance(2000);
    }
    let snapshot = last.unwrap();
    let history = &only_server(&snapshot).history;

    // The 10 s samples ending 0:68 through 11:00; 0:58 is over 10 minutes old.
    assert_eq!(history.len(), 61);
    assert_eq!(history[0].at, T0 + 68_000);
    assert_eq!(history[60].at, T0 + 660_000);
    let spike = history.iter().find(|s| s.at == T0 + 608_000).unwrap();
    assert_eq!(spike.cpu_percent, 50.0);
}

#[test]
fn growth_of_500_mb_over_two_minutes_is_a_leak() {
    let fake = Fake::new();
    next_dev(&fake);
    let mut engine = fake.engine();
    let mut scan_at = |memory_mb: u64| {
        fake.set_usage(220, memory_mb * MB, 0);
        let (snapshot, alerts) = engine.scan();
        fake.advance(30_000);
        (only_server(&snapshot).clone(), alerts)
    };

    let (first, _) = scan_at(1000);
    assert_eq!(first.status, ServerStatus::Running);
    for _ in 0..3 {
        scan_at(1500); // under two minutes of history: not yet a leak
    }
    let (leaking, alerts) = scan_at(1500);
    assert_eq!(leaking.status, ServerStatus::Attention);
    assert_eq!(
        leaking.clean_up,
        Some(CleanUpReason::Leaking { bytes: 500 * MB })
    );
    let kinds: Vec<AlertKind> = alerts.iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [AlertKind::Leaking]);

    // Once per episode, re-armed when growth drops under half the trigger.
    assert!(scan_at(1600).1.is_empty());
    assert!(scan_at(1200).1.is_empty());
    let (_, alerts) = scan_at(1550);
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].kind, AlertKind::Leaking);
    assert_eq!(alerts[0].memory, 1550 * MB);
}

#[test]
fn memory_alert_fires_once_per_episode() {
    let fake = Fake::new();
    next_dev(&fake);
    let mut engine = fake.engine();
    let mut alerts_at = |memory_mb: u64| {
        fake.set_usage(220, memory_mb * MB, 0);
        let (snapshot, alerts) = engine.scan();
        fake.advance(30_000);
        assert_eq!(only_server(&snapshot).memory, memory_mb * MB);
        alerts.iter().map(|a| a.kind).collect::<Vec<_>>()
    };

    // 2 GB is the default threshold; it must hold for 30 s.
    assert_eq!(alerts_at(2100), []);
    assert_eq!(alerts_at(2100), [AlertKind::OverThreshold]);
    assert_eq!(alerts_at(2100), []);
    // Still above 90% of the threshold, so the episode goes on.
    assert_eq!(alerts_at(1900), []);
    assert_eq!(alerts_at(2100), []);
    // Below 90% ends it.
    assert_eq!(alerts_at(1800), []);
    assert_eq!(alerts_at(2100), []);
    assert_eq!(alerts_at(2100), [AlertKind::OverThreshold]);
}

#[test]
fn over_the_threshold_needs_attention() {
    let fake = Fake::new();
    next_dev(&fake);
    fake.set_usage(220, 2048 * MB, 0);

    let (snapshot, _) = fake.engine().scan();

    assert_eq!(only_server(&snapshot).status, ServerStatus::Attention);
}

#[test]
fn deleted_worktree_is_idle_and_cleaned_up() {
    let fake = Fake::new();
    let gone = "/nonexistent/worktrees/fix-auth-timeout";
    fake.run(90, 1, "uvicorn", &["uvicorn", "app:main"], gone, HOUR);
    fake.listen(8000, 90, "127.0.0.1");

    let (snapshot, _) = fake.engine().scan();
    let server = only_server(&snapshot);

    assert!(!server.cwd_exists);
    assert_eq!(server.status, ServerStatus::Idle);
    assert_eq!(server.clean_up, Some(CleanUpReason::WorktreeDeleted));
}

#[test]
fn idle_for_four_hours_is_cleaned_up_unless_connected() {
    let fake = Fake::new();
    next_dev(&fake);
    let dir = project_dir();
    fake.run(300, 1, "python3", &["python3", "app.py"], dir, HOUR);
    fake.listen(8000, 300, "127.0.0.1");
    let mut engine = fake.engine();
    engine.scan();

    fake.advance(4 * HOUR);
    fake.world().connections = Some(HashMap::from([(8000, 1)]));
    let (snapshot, _) = engine.scan();

    let idle = &snapshot.servers[0];
    assert_eq!(idle.port, 3000);
    assert_eq!(idle.status, ServerStatus::Idle);
    assert_eq!(idle.clean_up, Some(CleanUpReason::Idle { seconds: 14_400 }));
    let connected = &snapshot.servers[1];
    assert_eq!(connected.connections, 1);
    assert_eq!(connected.status, ServerStatus::Running);
    assert_eq!(connected.clean_up, None);
}

#[test]
fn unknown_connections_never_make_a_server_idle() {
    let fake = Fake::new();
    next_dev(&fake);
    fake.world().connections = None;
    let mut engine = fake.engine();
    engine.scan();

    fake.advance(4 * HOUR);
    let (snapshot, _) = engine.scan();
    let server = only_server(&snapshot);

    assert_eq!(server.status, ServerStatus::Running);
    assert_eq!(server.clean_up, None);
}

#[test]
fn running_for_three_days_is_cleaned_up() {
    let fake = Fake::new();
    let dir = project_dir();
    fake.run(
        400,
        1,
        "ruby",
        &["ruby", "bin/rails", "server"],
        dir,
        72 * HOUR,
    );
    fake.listen(3000, 400, "127.0.0.1");

    let (snapshot, _) = fake.engine().scan();

    assert_eq!(
        only_server(&snapshot).clean_up,
        Some(CleanUpReason::LongRunning { seconds: 259_200 })
    );
}

/// `postgres -D data` listening on :5432, idle for three days.
fn postgres(fake: &Fake) {
    let args = ["postgres", "-D", "data"];
    fake.run(500, 1, "postgres", &args, project_dir(), 72 * HOUR);
    fake.listen(5432, 500, "127.0.0.1");
}

#[test]
fn protected_servers_are_never_cleaned_up_or_alerted() {
    let fake = Fake::new();
    let dir = project_dir();
    postgres(&fake);
    fake.set_usage(500, 3000 * MB, 0);
    // A redis child protects the dev server that started it.
    next_dev(&fake);
    fake.run(240, 200, "redis-server", &["redis-server"], dir, HOUR);
    fake.run(300, 1, "python3", &["python3", "app.py"], dir, HOUR);
    fake.listen(8000, 300, "127.0.0.1");
    let mut engine = fake.engine();
    engine.scan();

    fake.advance(5 * HOUR);
    let (snapshot, alerts) = engine.scan();
    let [next, database, python] = &snapshot.servers[..] else {
        panic!("{:#?}", snapshot.servers);
    };

    assert!(next.protected);
    assert_eq!(next.clean_up, None);
    assert!(database.protected);
    assert_eq!(database.clean_up, None);
    assert!(!python.protected);
    assert_eq!(
        python.clean_up,
        Some(CleanUpReason::Idle { seconds: 18_000 })
    );
    assert!(alerts.is_empty());
}

#[test]
fn stop_and_restart_refuse_a_protected_tree_until_confirmed() {
    let fake = Fake::new();
    postgres(&fake);
    let root = fake.proc_ref(500);
    let mut engine = fake.engine();
    let stop = |confirm_protected| Call::Stop {
        port: 5432,
        root,
        force: false,
        confirm_protected,
    };
    let restart = Call::Restart {
        port: 5432,
        root,
        confirm_protected: false,
    };
    let refusal = "postgres :5432 is protected; send confirm_protected to stop it anyway";

    assert_eq!(engine.call(&stop(false)), Err(refusal.into()));
    let refusal = "postgres :5432 is protected; send confirm_protected to restart it anyway";
    assert_eq!(engine.call(&restart), Err(refusal.into()));
    assert!(fake.world().signals.is_empty());

    assert_eq!(engine.call(&stop(true)), Ok(()));
    assert_eq!(fake.world().signals, [(500, false)]);
}

#[test]
fn stop_refuses_a_server_with_a_protected_child() {
    let fake = Fake::new();
    next_dev(&fake);
    fake.run(
        240,
        200,
        "redis-server.exe",
        &["redis-server"],
        project_dir(),
        HOUR,
    );
    let stop = Call::Stop {
        port: 3000,
        root: fake.proc_ref(200),
        force: true,
        confirm_protected: false,
    };

    let refusal = "redis-server.exe :3000 is protected; send confirm_protected to stop it anyway";
    assert_eq!(fake.engine().call(&stop), Err(refusal.into()));
    assert!(fake.world().signals.is_empty());
}

#[test]
fn stop_terminates_leaves_first() {
    let fake = Fake::new();
    next_dev(&fake);
    let root = fake.proc_ref(200);

    let stop = Call::Stop {
        port: 3000,
        root,
        force: false,
        confirm_protected: false,
    };
    assert_eq!(fake.engine().call(&stop), Ok(()));

    let signals = fake.world().signals.clone();
    assert_eq!(
        signals,
        [(230, false), (220, false), (210, false), (200, false)]
    );
}

#[test]
fn a_later_scan_kills_what_ignores_terminate_for_three_seconds() {
    let fake = Fake::new();
    next_dev(&fake);
    fake.world().stubborn.insert(220);
    let root = fake.proc_ref(200);
    let mut engine = fake.engine();

    let stop = Call::Stop {
        port: 3000,
        root,
        force: false,
        confirm_protected: false,
    };
    assert_eq!(engine.call(&stop), Ok(()));
    let terminated = [(230, false), (220, false), (210, false), (200, false)];
    assert_eq!(fake.world().signals, terminated);

    fake.advance(2000);
    engine.scan();
    assert_eq!(fake.world().signals.len(), 4);
    assert!(engine.pending());

    fake.advance(1000);
    engine.scan();
    assert_eq!(fake.world().signals.last(), Some(&(220, true)));
    engine.scan();
    assert!(!engine.pending());
    assert!(fake.world().procs.iter().all(|p| p.info.proc.pid == 100));
}

#[test]
fn interrupted_processes_get_no_terminate_and_are_killed_after_three_seconds() {
    let fake = Fake::new();
    next_dev(&fake);
    // 230 runs on another console, so the interrupt doesn't reach it.
    fake.world().interrupts = HashSet::from([200, 210, 220]);
    let root = fake.proc_ref(200);
    let mut engine = fake.engine();

    let stop = Call::Stop {
        port: 3000,
        root,
        force: false,
        confirm_protected: false,
    };
    assert_eq!(engine.call(&stop), Ok(()));
    assert_eq!(fake.world().interrupted, [[220, 210, 200]]);
    assert_eq!(fake.world().signals, [(230, false)]);

    fake.advance(3000);
    engine.scan();
    let killed = [(230, false), (220, true), (210, true), (200, true)];
    assert_eq!(fake.world().signals, killed);
}

#[test]
fn stop_refuses_a_reused_pid() {
    let fake = Fake::new();
    next_dev(&fake);
    let mut root = fake.proc_ref(200);
    root.started_at -= 1000;

    let stop = Call::Stop {
        port: 3000,
        root,
        force: true,
        confirm_protected: false,
    };
    assert!(fake.engine().call(&stop).is_err());

    assert!(fake.world().signals.is_empty());
    assert_eq!(fake.world().procs.len(), 5);
}

#[cfg(unix)]
#[test]
fn restart_reruns_the_launch_command_with_its_environment() {
    let dir = std::env::temp_dir().join(format!("ppm-restart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir_str = dir.to_str().unwrap();
    let fake = Fake::new();
    fake.run(200, 1, "node", &["npm run dev"], dir_str, HOUR);
    let script = r#"printf %s "$PPM_GREETING" > restarted.txt"#;
    fake.run(210, 200, "sh", &["sh", "-c", script], dir_str, HOUR);
    fake.run(220, 210, "node", &["next-server (v15.1.0)"], dir_str, HOUR);
    fake.listen(3000, 220, "127.0.0.1");
    fake.world().procs[1].env = vec![("PPM_GREETING".into(), "hello".into())];
    let root = fake.proc_ref(200);

    let mut engine = fake.engine();
    let restart = Call::Restart {
        port: 3000,
        root,
        confirm_protected: false,
    };
    assert_eq!(engine.call(&restart), Ok(()));
    // The next scan sees the old tree gone and the port free.
    engine.scan();
    assert!(!engine.pending());

    let output = dir.join("restarted.txt");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !output.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "hello");
    std::fs::remove_dir_all(&dir).unwrap();
}
