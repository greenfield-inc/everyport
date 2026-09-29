//! `everyport remote add|list|rm`: the machines in `machines.toml`, shared with
//! the desktop app, next to the ones everyport discovers.

use crate::machine::{machines_path, RUNTIME};
use everyport::client::install;
use everyport::client::machines::{self, Machine, Via};
use everyport::client::Connection;
use everyport::client::{check, discover};
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

/// How long `everyport remote list` waits for each machine to answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

fn other(error: anyhow::Error) -> io::Error {
    io::Error::other(format!("{error:#}"))
}

pub fn add(name: String, code: Option<String>, command: Vec<String>) -> io::Result<ExitCode> {
    let via = match code {
        Some(code) => match Connection::from_code(&code).map_err(other)? {
            Connection::Http { url, token } => Via::Url { url, token },
            _ => unreachable!("a code is always a URL"),
        },
        None => Via::Command { command },
    };
    let path = machines_path().map_err(other)?;
    let mut saved = machines::load(&path).map_err(other)?;
    let existed = saved.iter().any(|m| m.name == name);
    saved.retain(|m| m.name != name);
    saved.push(Machine {
        name: name.clone(),
        via,
    });
    machines::save(&path, &saved).map_err(other)?;
    let verb = if existed { "Updated" } else { "Added" };
    println!("{verb} {name}. Run `everyport --on {name}` to see its servers.");
    Ok(ExitCode::SUCCESS)
}

pub fn rm(name: &str) -> io::Result<ExitCode> {
    let path = machines_path().map_err(other)?;
    let mut saved = machines::load(&path).map_err(other)?;
    let before = saved.len();
    saved.retain(|m| m.name != name);
    if saved.len() == before {
        eprintln!("everyport: no saved machine named {name}");
        return Ok(ExitCode::FAILURE);
    }
    machines::save(&path, &saved).map_err(other)?;
    println!("Removed {name}.");
    Ok(ExitCode::SUCCESS)
}

/// Checks the way to one machine, or to every saved and discovered one, step
/// by step. Each stops at its first failed step and says how to fix it.
pub fn doctor(name: Option<&str>) -> io::Result<ExitCode> {
    let saved = machines::load(&machines_path().map_err(other)?).map_err(other)?;
    let reports = RUNTIME.block_on(async {
        let found = discover::all().await;
        let mut known: Vec<Machine> = saved;
        for f in &found {
            if !known.iter().any(|m| m.name == f.machine.name) {
                known.push(f.machine.clone());
            }
        }
        if let Some(name) = name {
            known.retain(|m| m.name == name);
        }
        let checks: Vec<_> = known
            .into_iter()
            .map(|machine| {
                let hint = discover::hint(&found, &machine.via);
                tokio::spawn(async move {
                    let report = check::check(&machine.name, &machine.via, &hint).await;
                    (machine, report)
                })
            })
            .collect();
        let mut reports = Vec::new();
        for check in checks {
            reports.extend(check.await);
        }
        reports
    });
    if let (Some(name), true) = (name, reports.is_empty()) {
        eprintln!("everyport: no machine named {name}. See `everyport remote list`.");
        return Ok(ExitCode::FAILURE);
    }
    if reports.is_empty() {
        println!(
            "No other machines yet. Add one with `everyport remote add devbox -- ssh devbox`."
        );
    }
    let mut ok = true;
    for (i, (machine, report)) in reports.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{} ({})", machine.name, connection(&machine.via));
        println!("{report}");
        ok &= report.ok();
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Saved machines, then discovered ones not saved under the same name, each
/// with the everyport version installed there.
pub fn list() -> io::Result<ExitCode> {
    let saved = machines::load(&machines_path().map_err(other)?).map_err(other)?;
    let rows = RUNTIME.block_on(async {
        let found = discover::all().await;
        let mut rows: Vec<(Machine, String)> =
            saved.into_iter().map(|m| (m, "saved".into())).collect();
        for found in found {
            if !rows.iter().any(|(m, _)| m.name == found.machine.name) {
                rows.push((found.machine, found.source.label()));
            }
        }
        let probes: Vec<_> = rows
            .iter()
            .map(|(machine, _)| tokio::spawn(installed(machine.via.clone())))
            .collect();
        let mut table = Vec::new();
        for ((machine, source), probe) in rows.into_iter().zip(probes) {
            let everyport = probe.await.unwrap_or_else(|_| "unreachable".into());
            table.push([machine.name, connection(&machine.via), source, everyport]);
        }
        table
    });
    if rows.is_empty() {
        println!("No machines yet. Add one with `everyport remote add devbox -- ssh devbox`.");
        return Ok(ExitCode::SUCCESS);
    }
    let headers = ["NAME", "CONNECTION", "FROM", "EVERYPORT"].map(String::from);
    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            rows.iter()
                .chain([&headers])
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in [&headers].into_iter().chain(&rows) {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        println!("{}", cells.join("  ").trim_end());
    }
    Ok(ExitCode::SUCCESS)
}

/// The everyport version on the machine, "not installed", or "unreachable". ssh
/// never prompts here, so many machines can be probed at once.
async fn installed(via: Via) -> String {
    let mut prefix = match via {
        Via::Url { .. } => return "everyport serve".into(),
        Via::Command { command } => command,
    };
    let ssh = prefix.first().map(Path::new).and_then(Path::file_stem);
    if ssh.is_some_and(|s| s == "ssh") {
        let options = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"];
        prefix.splice(1..1, options.map(String::from));
    }
    match tokio::time::timeout(PROBE_TIMEOUT, install::probe(&prefix)).await {
        Ok(Ok(probe)) => probe.installed.unwrap_or_else(|| "not installed".into()),
        _ => "unreachable".into(),
    }
}

fn connection(via: &Via) -> String {
    match via {
        Via::Url { url, .. } => url.clone(),
        Via::Command { command } => command
            .iter()
            .map(|arg| {
                if arg.is_empty() || arg.contains(char::is_whitespace) {
                    format!("'{arg}'")
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
