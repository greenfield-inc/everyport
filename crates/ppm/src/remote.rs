//! `ppm remote add|list|rm`: the machines in `machines.toml`, shared with
//! the desktop app, next to the ones ppm discovers.

use crate::machine::{machines_path, RUNTIME};
use ppm_client::discover::{self, Source};
use ppm_client::install;
use ppm_client::machines::{self, Machine, Via};
use ppm_client::Connection;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

/// How long `ppm remote list` waits for each machine to answer.
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
    println!("{verb} {name}. Run `ppm --on {name}` to see its servers.");
    Ok(ExitCode::SUCCESS)
}

pub fn rm(name: &str) -> io::Result<ExitCode> {
    let path = machines_path().map_err(other)?;
    let mut saved = machines::load(&path).map_err(other)?;
    let before = saved.len();
    saved.retain(|m| m.name != name);
    if saved.len() == before {
        eprintln!("ppm: no saved machine named {name}");
        return Ok(ExitCode::FAILURE);
    }
    machines::save(&path, &saved).map_err(other)?;
    println!("Removed {name}.");
    Ok(ExitCode::SUCCESS)
}

/// Saved machines, then discovered ones not saved under the same name, each
/// with the ppm version installed there.
pub fn list() -> io::Result<ExitCode> {
    let saved = machines::load(&machines_path().map_err(other)?).map_err(other)?;
    let rows = RUNTIME.block_on(async {
        let found = discover::all().await;
        let mut rows: Vec<(Machine, &str)> = saved.into_iter().map(|m| (m, "saved")).collect();
        for found in found {
            if !rows.iter().any(|(m, _)| m.name == found.machine.name) {
                let source = match found.source {
                    Source::SshConfig => "ssh config",
                    Source::Pane => "Pane",
                    Source::Wsl => "WSL",
                };
                rows.push((found.machine, source));
            }
        }
        let probes: Vec<_> = rows
            .iter()
            .map(|(machine, _)| tokio::spawn(installed(machine.via.clone())))
            .collect();
        let mut table = Vec::new();
        for ((machine, source), probe) in rows.into_iter().zip(probes) {
            let ppm = probe.await.unwrap_or_else(|_| "unreachable".into());
            table.push([machine.name, connection(&machine.via), source.into(), ppm]);
        }
        table
    });
    if rows.is_empty() {
        println!("No machines yet. Add one with `ppm remote add devbox -- ssh devbox`.");
        return Ok(ExitCode::SUCCESS);
    }
    let headers = ["NAME", "CONNECTION", "FROM", "PPM"].map(String::from);
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

/// The ppm version on the machine, "not installed", or "unreachable". ssh
/// never prompts here, so many machines can be probed at once.
async fn installed(via: Via) -> String {
    let mut prefix = match via {
        Via::Url { .. } => return "ppm serve".into(),
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
