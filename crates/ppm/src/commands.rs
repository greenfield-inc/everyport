//! The one-shot commands: list, watch, stop, restart, open, clean and doctor.

use crate::format;
use crate::hub::Hub;
use crate::palette::Palette;
use ppm_core::engine::Engine;
use ppm_core::platform;
use ppm_core::protocol::{Call, Config, Event, Os, Server, ServerStatus, Snapshot};
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

pub fn engine() -> Engine {
    Engine::new(platform::native(), Config::default())
}

/// CPU is measured between two scans.
fn scan_twice(engine: &mut Engine) -> Snapshot {
    engine.scan();
    std::thread::sleep(Duration::from_millis(500));
    engine.scan().0
}

pub fn list(json: bool) -> io::Result<ExitCode> {
    let snapshot = scan_twice(&mut engine());
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &snapshot)?;
        writeln!(out)?;
    } else if snapshot.servers.is_empty() {
        let config = Config::default();
        writeln!(
            out,
            "Nothing listening on ports {}-{}.",
            config.min_port, config.max_port
        )?;
    } else {
        write_table(&mut out, &snapshot)?;
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

/// One snapshot per line, only when something changed.
pub fn watch() -> io::Result<ExitCode> {
    let hub = Hub::start(engine());
    let (events, rx) = mpsc::channel();
    hub.subscribe(events);
    let mut out = io::stdout().lock();
    for event in rx {
        if let Event::Snapshot(snapshot) = event {
            serde_json::to_writer(&mut out, &snapshot)?;
            writeln!(out)?;
            out.flush()?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Scans until every stop and restart has finished, and returns the last snapshot.
fn finish(engine: &mut Engine) -> Snapshot {
    loop {
        let (snapshot, _) = engine.scan();
        if !engine.pending() {
            return snapshot;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn find(snapshot: &Snapshot, port: u16) -> Option<&Server> {
    snapshot.servers.iter().find(|server| server.port == port)
}

fn nothing_on(port: u16) -> ExitCode {
    eprintln!("ppm: nothing listening on :{port}");
    ExitCode::FAILURE
}

fn outcome(result: Result<(), String>, done: String) -> ExitCode {
    match result {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ppm: {error}");
            ExitCode::FAILURE
        }
    }
}

pub fn stop(port: u16, force: bool) -> ExitCode {
    let mut engine = engine();
    let (snapshot, _) = engine.scan();
    let Some(server) = find(&snapshot, port) else {
        return nothing_on(port);
    };
    let root = server.root;
    let result = engine
        .call(&Call::Stop { port, root, force })
        .and_then(|()| match find(&finish(&mut engine), port) {
            Some(server) if server.root == root => Err(format!(":{port} is still running")),
            _ => Ok(()),
        });
    outcome(result, format!("Stopped {} :{port}", server.project.name))
}

pub fn restart(port: u16) -> ExitCode {
    let mut engine = engine();
    let (snapshot, _) = engine.scan();
    let Some(server) = find(&snapshot, port) else {
        return nothing_on(port);
    };
    let result = engine.call(&Call::Restart {
        port,
        root: server.root,
    });
    if result.is_ok() {
        finish(&mut engine);
    }
    let command = server.command.as_deref().unwrap_or("its command");
    outcome(result, format!("Restarted :{port} with {command}"))
}

pub fn open(port: u16) -> ExitCode {
    let url = format!("http://localhost:{port}");
    outcome(
        open::that_detached(&url).map_err(|e| e.to_string()),
        format!("Opened {url}"),
    )
}

pub fn clean(yes: bool) -> io::Result<ExitCode> {
    let mut engine = engine();
    let snapshot = scan_twice(&mut engine);
    let picks: Vec<&Server> = snapshot
        .servers
        .iter()
        .filter(|s| format::preselected(s))
        .collect();
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
        if !io::stdin().is_terminal() {
            eprintln!("ppm: run `ppm clean --yes` to stop these without asking");
            return Ok(ExitCode::FAILURE);
        }
        print!("Stop {count} and free {}? [y/N] ", format::bytes(memory));
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Ok(ExitCode::SUCCESS);
        }
    }
    let mut failed = false;
    for server in &picks {
        let call = Call::Stop {
            port: server.port,
            root: server.root,
            force: false,
        };
        if let Err(error) = engine.call(&call) {
            eprintln!("ppm: :{}: {error}", server.port);
            failed = true;
        }
    }
    finish(&mut engine);
    println!("Stopped {count}, freeing {}.", format::bytes(memory));
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
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
                    "  {} belong to other users or the system; ppm shows their port and process name only",
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
