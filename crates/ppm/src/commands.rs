//! The one-shot commands: list, watch, stop, restart, open, clean and doctor.
//! Each runs against a [`Machine`], this one or another, through the protocol.

use crate::format;
use crate::machine::{self, Feed, Install, Machine, RUNTIME};
use crate::palette::Palette;
use ppm_client::forward::forward;
use ppm_core::engine::Engine;
use ppm_core::platform;
use ppm_core::protocol::{
    AutoKill, Call, Config, Event, Os, ProcRef, Server, ServerStatus, Snapshot,
};
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

/// The user's settings from `config.toml`, which the desktop app writes.
pub fn config() -> io::Result<Config> {
    match ppm_core::config::path() {
        Some(path) => ppm_core::config::load(&path),
        None => Ok(Config::default()),
    }
}

/// The scanner, with the user's settings. `ppm` never auto-kills on its
/// own: only a client that asks through `configure`, such as the desktop
/// app for this computer, turns it on.
pub fn engine() -> io::Result<Engine> {
    let config = Config {
        auto_kill: AutoKill::Off,
        ..config()?
    };
    Ok(Engine::new(platform::native(), config))
}

/// True when a person can answer a question: stdin and stderr are both a
/// terminal. Stdout may be redirected, as in `ppm list --json > out.json`.
pub fn can_ask() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

/// Asks a yes or no question on stderr, keeping stdout for output. No is
/// the default.
pub fn confirm(question: &str) -> io::Result<bool> {
    eprint!("{question} [y/N] ");
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn lost(machine: &Machine, error: String) -> io::Error {
    match &machine.name {
        Some(name) => io::Error::other(format!("lost {name}: {error}")),
        None => io::Error::other(error),
    }
}

/// The next snapshot. A lost connection ends a one-shot command.
fn next_snapshot(machine: &Machine) -> io::Result<Snapshot> {
    loop {
        match machine.recv() {
            Some(Feed::Event(Event::Snapshot(snapshot))) => return Ok(snapshot),
            Some(Feed::Event(_)) => {}
            Some(Feed::Lost(error)) => return Err(lost(machine, error)),
            None => return Err(lost(machine, "the connection closed".into())),
        }
    }
}

/// CPU is measured between two scans, so this asks for a second one.
fn measured_snapshot(machine: &mut Machine) -> io::Result<Snapshot> {
    let first = next_snapshot(machine)?;
    std::thread::sleep(Duration::from_millis(500));
    machine.call(Call::Refresh);
    loop {
        let snapshot = next_snapshot(machine)?;
        if snapshot.taken_at > first.taken_at {
            return Ok(snapshot);
        }
    }
}

pub fn list(mut machine: Machine, json: bool) -> io::Result<ExitCode> {
    let snapshot = measured_snapshot(&mut machine)?;
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &snapshot)?;
        writeln!(out)?;
    } else if snapshot.servers.is_empty() && snapshot.other_ports.is_empty() {
        // The range this machine scans: its own settings, or the defaults.
        let config = if machine.name.is_none() {
            config()?
        } else {
            Config::default()
        };
        writeln!(
            out,
            "Nothing listening on ports {}-{}.",
            config.min_port, config.max_port
        )?;
    } else {
        if !snapshot.servers.is_empty() {
            write_table(&mut out, &snapshot)?;
        }
        if !snapshot.other_ports.is_empty() {
            if !snapshot.servers.is_empty() {
                writeln!(out)?;
            }
            writeln!(out, "Other ports")?;
            for port in &snapshot.other_ports {
                writeln!(out, "{}", format::other_port(port))?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn write_table(out: &mut impl Write, snapshot: &Snapshot) -> io::Result<()> {
    let palette = Palette::detect();
    let headers = ["PORT", "NAME", "BRANCH", "MEMORY", "CPU", "UP", "SESSION"];
    let right_aligned = [3, 4, 5];
    let rows: Vec<[String; 7]> = snapshot
        .servers
        .iter()
        .map(|server| {
            [
                format!(":{}", server.port),
                server.project.name.clone(),
                server.project.branch.clone().unwrap_or_default(),
                format::bytes(server.memory),
                format::percent(server.cpu_percent),
                format::uptime(server, snapshot.taken_at)
                    .map(format::short_duration)
                    .unwrap_or_default(),
                server.agent.as_ref().map_or(String::new(), |agent| {
                    let kind = format::agent(agent.kind);
                    agent
                        .title
                        .as_ref()
                        .map_or(kind.to_string(), |title| format!("{kind} · {title}"))
                }),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain([headers[column].len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let cells = |row: &[String]| -> Vec<String> {
        row.iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = widths[column];
                if right_aligned.contains(&column) {
                    format!("{cell:>width$}")
                } else {
                    format!("{cell:<width$}")
                }
            })
            .collect()
    };
    let (dim, reset) = if palette.has_color() {
        ("\x1b[2m", "\x1b[0m")
    } else {
        ("", "")
    };
    let headers = headers.map(String::from);
    writeln!(out, "{dim}{}{reset}", cells(&headers).join("  ").trim_end())?;
    for (index, (server, row)) in snapshot.servers.iter().zip(&rows).enumerate() {
        let mut cells = cells(row);
        if palette.has_color() {
            cells[0] = format!("{}{}{reset}", palette.ansi(palette.port(index)), cells[0]);
            if server.status == ServerStatus::Attention {
                cells[3] = format!("{}{}{reset}", palette.ansi(palette.amber()), cells[3]);
            }
        }
        writeln!(out, "{}", cells.join("  ").trim_end())?;
    }
    Ok(())
}

/// One snapshot per line, only when something changed. Keeps going through
/// reconnects to another machine.
pub fn watch(machine: Machine) -> io::Result<ExitCode> {
    let mut out = io::stdout().lock();
    while let Some(feed) = machine.recv() {
        match feed {
            Feed::Event(Event::Snapshot(snapshot)) => {
                serde_json::to_writer(&mut out, &snapshot)?;
                writeln!(out)?;
                out.flush()?;
            }
            Feed::Event(_) => {}
            Feed::Lost(error) => eprintln!("ppm: {}, reconnecting", lost(&machine, error)),
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// How long the engine lets a server quit before it kills what's left.
const GRACE: Duration = Duration::from_secs(3);
/// How long a kill takes to show in a scan.
const SETTLE: Duration = Duration::from_secs(5);
/// How long a restarted server gets to listen on its port again.
const COME_BACK: Duration = Duration::from_secs(10);

struct Settled {
    done: bool,
    last: Snapshot,
    /// Errors from the requests, by request id.
    errors: HashMap<u64, String>,
}

/// Asks for a scan every 250 ms, which also moves stops and restarts along,
/// until `done` holds for the latest snapshot and the request errors so far,
/// or `limit` passes.
fn settle(
    machine: &mut Machine,
    ids: &[u64],
    mut last: Snapshot,
    limit: Duration,
    done: impl Fn(&Snapshot, &HashMap<u64, String>) -> bool,
) -> io::Result<Settled> {
    let deadline = Instant::now() + limit;
    let mut errors = HashMap::new();
    while !done(&last, &errors) {
        if Instant::now() >= deadline {
            return Ok(Settled {
                done: false,
                last,
                errors,
            });
        }
        match machine.recv_timeout(Duration::from_millis(250)) {
            Ok(Feed::Event(Event::Snapshot(snapshot))) => last = snapshot,
            Ok(Feed::Event(Event::Result(result))) => {
                if let Some(error) = result.error.filter(|_| ids.contains(&result.id)) {
                    errors.insert(result.id, error);
                }
            }
            Ok(Feed::Event(_)) => {}
            Ok(Feed::Lost(error)) => return Err(lost(machine, error)),
            Err(RecvTimeoutError::Timeout) => {
                machine.call(Call::Refresh);
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(lost(machine, "the connection closed".into()))
            }
        }
    }
    Ok(Settled {
        done: true,
        last,
        errors,
    })
}

/// Whether any server still runs from the tree under `root`.
fn running(snapshot: &Snapshot, root: ProcRef) -> bool {
    snapshot.servers.iter().any(|server| server.root == root)
}

fn find(snapshot: &Snapshot, port: u16) -> Option<&Server> {
    snapshot.servers.iter().find(|server| server.port == port)
}

fn failed(error: String) -> ExitCode {
    eprintln!("ppm: {error}");
    ExitCode::FAILURE
}

/// Stop and restart leave a protected server alone unless told otherwise.
fn refuse_protected(server: &Server, command: &str) -> ExitCode {
    let port = server.port;
    failed(format!(
        "{} :{port} is protected. Run `ppm {command} {port} --protected` to {command} it anyway.",
        server.process_name
    ))
}

/// `force` kills at once and implies `protected`.
pub fn stop(mut machine: Machine, port: u16, force: bool, protected: bool) -> io::Result<ExitCode> {
    let confirm_protected = force || protected;
    let snapshot = next_snapshot(&machine)?;
    let Some(server) = find(&snapshot, port).cloned() else {
        return Ok(failed(format!("nothing listening on :{port}")));
    };
    if server.protected && !confirm_protected {
        return Ok(refuse_protected(&server, "stop"));
    }
    let root = server.root;
    let start = Instant::now();
    let id = machine.call(Call::Stop {
        port,
        root,
        force,
        confirm_protected,
    });
    let settled = settle(
        &mut machine,
        &[id],
        snapshot,
        GRACE + SETTLE,
        |s, errors| errors.contains_key(&id) || !running(s, root),
    )?;
    if let Some(error) = settled.errors.get(&id) {
        return Ok(failed(error.clone()));
    }
    if !settled.done {
        return Ok(failed(format!(
            ":{port} is still running, even after it was killed"
        )));
    }
    let how = if force {
        ", killed"
    } else if start.elapsed() >= GRACE {
        ", killed after it ignored the request to quit for 3 s"
    } else {
        ""
    };
    println!("Stopped {} :{port}{how}", server.project.name);
    machine.close();
    Ok(ExitCode::SUCCESS)
}

pub fn restart(mut machine: Machine, port: u16, protected: bool) -> io::Result<ExitCode> {
    let snapshot = next_snapshot(&machine)?;
    let Some(server) = find(&snapshot, port).cloned() else {
        return Ok(failed(format!("nothing listening on :{port}")));
    };
    if server.protected && !protected {
        return Ok(refuse_protected(&server, "restart"));
    }
    let root = server.root;
    let id = machine.call(Call::Restart {
        port,
        root,
        confirm_protected: protected,
    });
    let settled = settle(
        &mut machine,
        &[id],
        snapshot,
        GRACE + COME_BACK,
        |s, errors| errors.contains_key(&id) || find(s, port).is_some_and(|s| s.root != root),
    )?;
    if let Some(error) = settled.errors.get(&id) {
        return Ok(failed(error.clone()));
    }
    if settled.done {
        let command = server.command.as_deref().unwrap_or("its command");
        println!("Restarted :{port} with {command}");
        machine.close();
        return Ok(ExitCode::SUCCESS);
    }
    let log = format!("port-process-manager/port-{port}.log");
    let error = if running(&settled.last, root) {
        format!("the old server on :{port} is still running")
    } else if machine.is_local() {
        format!(
            "nothing is listening on :{port} {} s after the restart; its output is in {}",
            COME_BACK.as_secs(),
            std::env::temp_dir().join(log).display()
        )
    } else {
        format!(
            "nothing is listening on :{port} {} s after the restart; its output is in {log} in the temp folder on {}",
            COME_BACK.as_secs(),
            machine.label()
        )
    };
    machine.close();
    Ok(failed(error))
}

/// Opens the server in the browser. A server on another machine is
/// forwarded first, and the forward stays open until Ctrl-C.
pub fn open(on: Option<&str>, install: Install, port: u16) -> io::Result<ExitCode> {
    let Some(name) = on else {
        return Ok(open_url(&format!("http://localhost:{port}")));
    };
    let connection = machine::connection(name, install).map_err(io::Error::other)?;
    let forwarded = match RUNTIME.block_on(forward(&connection, port)) {
        Ok(forwarded) => forwarded,
        Err(error) => return Ok(failed(format!("{error:#}"))),
    };
    if !forwarded.is_tunnel() {
        return Ok(open_url(&forwarded.url));
    }
    if let Err(error) = open::that_detached(&forwarded.url) {
        return Ok(failed(error.to_string()));
    }
    println!(
        "Opened {}, forwarded from :{port} on {name}. Press Ctrl-C to stop forwarding.",
        forwarded.url
    );
    RUNTIME.block_on(tokio::signal::ctrl_c())?;
    Ok(ExitCode::SUCCESS)
}

fn open_url(url: &str) -> ExitCode {
    match open::that_detached(url) {
        Ok(()) => {
            println!("Opened {url}");
            ExitCode::SUCCESS
        }
        Err(error) => failed(error.to_string()),
    }
}

pub fn clean(mut machine: Machine, yes: bool) -> io::Result<ExitCode> {
    let snapshot = measured_snapshot(&mut machine)?;
    let picks: Vec<Server> = snapshot
        .servers
        .iter()
        .filter(|s| format::preselected(s))
        .cloned()
        .collect();
    let protected: Vec<String> = snapshot
        .servers
        .iter()
        .filter(|s| s.protected)
        .map(|s| format!("{} :{}", s.process_name, s.port))
        .collect();
    if !protected.is_empty() {
        println!("Skipping protected: {}", protected.join(", "));
    }
    if picks.is_empty() {
        println!("Nothing to clean up.");
        return Ok(ExitCode::SUCCESS);
    }
    for server in &picks {
        let reason = server
            .clean_up
            .as_ref()
            .map(format::reason)
            .unwrap_or_default();
        println!(
            ":{:<6}{:<24}{:>9}   {reason}",
            server.port,
            server.project.name,
            format::bytes(server.memory)
        );
    }
    let memory: u64 = picks.iter().map(|s| s.memory).sum();
    let count = format::plural(picks.len(), "server", "servers");
    if !yes {
        if !can_ask() {
            eprintln!("ppm: run `ppm clean --yes` to stop these without asking");
            return Ok(ExitCode::FAILURE);
        }
        if !confirm(&format!("Stop {count} and free {}?", format::bytes(memory)))? {
            return Ok(ExitCode::SUCCESS);
        }
    }
    let asked: Vec<(u64, &Server)> = picks
        .iter()
        .map(|server| {
            let id = machine.call(Call::Stop {
                port: server.port,
                root: server.root,
                force: false,
                confirm_protected: false,
            });
            (id, server)
        })
        .collect();
    let ids: Vec<u64> = asked.iter().map(|(id, _)| *id).collect();
    let settled = settle(&mut machine, &ids, snapshot, GRACE + SETTLE, |s, errors| {
        asked
            .iter()
            .all(|(id, server)| errors.contains_key(id) || !running(s, server.root))
    })?;
    let mut ok = true;
    let mut stopped = Vec::new();
    for (id, server) in &asked {
        if let Some(error) = settled.errors.get(id) {
            eprintln!("ppm: :{}: {error}", server.port);
            ok = false;
        } else if running(&settled.last, server.root) {
            eprintln!(
                "ppm: :{} is still running, even after it was killed",
                server.port
            );
            ok = false;
        } else {
            stopped.push(*server);
        }
    }
    println!(
        "Stopped {}, freeing {}.",
        format::plural(stopped.len(), "server", "servers"),
        format::bytes(stopped.iter().map(|s| s.memory).sum())
    );
    machine.close();
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

pub fn doctor(config_dir: Option<std::path::PathBuf>) -> ExitCode {
    let host = ppm_core::host::info();
    let os = match host.os {
        Os::Macos => "macOS",
        Os::Linux => "Linux",
        Os::Windows => "Windows",
    };
    println!(
        "ppm {} · protocol {} · {os} {} · {} cores",
        env!("CARGO_PKG_VERSION"),
        ppm_core::protocol::PROTOCOL_VERSION,
        host.arch,
        host.cores
    );
    let platform = platform::native();
    let mut ok = true;
    let mut check = |pass: bool, text: String| {
        println!("{} {text}", if pass { "✓" } else { "✗" });
        ok &= pass;
    };

    match platform.listeners() {
        Ok(listeners) => {
            let hidden = listeners
                .iter()
                .filter(|l| platform.details(l.pid, &[]).is_none())
                .count();
            check(true, format!("Listening ports: {} found", listeners.len()));
            if hidden > 0 {
                println!(
                    "  {} belong to other users or the system; ppm shows fewer details for them",
                    format::plural(hidden, "port", "ports")
                );
            }
        }
        Err(error) => check(false, format!("Listening ports: {error}")),
    }
    match platform.processes() {
        Ok(processes) => check(
            !processes.is_empty(),
            format!("Processes: {} found", processes.len()),
        ),
        Err(error) => check(false, format!("Processes: {error}")),
    }
    let own = platform.details(std::process::id(), &[]);
    check(
        own.as_ref()
            .is_some_and(|d| d.cwd.is_some() && !d.args.is_empty()),
        "Folder and command of your own processes".into(),
    );
    check(
        platform
            .usage(std::process::id())
            .is_some_and(|u| u.memory > 0),
        "Memory and CPU of your own processes".into(),
    );
    match config_dir {
        Some(dir) => {
            let writable = std::fs::create_dir_all(&dir).is_ok() && tempfile_check(&dir);
            check(writable, format!("Settings folder: {}", dir.display()));
            if let Err(error) = config() {
                check(false, format!("Settings: {error}"));
            }
        }
        None => check(
            false,
            "Settings folder: no config folder for this user".into(),
        ),
    }
    if cfg!(target_os = "linux") {
        println!(
            "  Desktop app window blank on NVIDIA? Start it with WEBKIT_DISABLE_DMABUF_RENDERER=1."
        );
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn tempfile_check(dir: &std::path::Path) -> bool {
    let path = dir.join(".ppm-doctor");
    let written = std::fs::write(&path, b"ok").is_ok();
    let _ = std::fs::remove_file(&path);
    written
}
