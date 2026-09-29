//! Stop and restart. Every target is checked by ProcRef, so a reused pid is
//! never signalled. Calls only send the first signal; `scan` finishes them,
//! so snapshots and alerts keep flowing while a server shuts down.

use super::tree::Table;
use super::{command, Engine};
use crate::platform::Listener;
use crate::protocol::ProcRef;
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// How long a server gets to exit after a terminate before it is killed.
const GRACE_MS: u64 = 3_000;
/// How long a stop or restart waits in all. A restart then relaunches even
/// if the port still looks taken.
const GIVE_UP_MS: u64 = 8_000;

/// A stop or restart whose tree hasn't exited yet.
pub(super) struct Pending {
    targets: Vec<ProcRef>,
    /// When to kill what ignored the terminate. None once killed.
    kill_at: Option<u64>,
    give_up_at: u64,
    relaunch: Option<Relaunch>,
}

struct Relaunch {
    port: u16,
    command: String,
    dir: String,
    environment: Option<Vec<(String, String)>>,
}

impl Engine {
    /// Whether a stop or restart is still in progress. Each `scan` advances
    /// it; a one-shot caller scans until this is false.
    pub fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Terminates the tree under `root`, leaves first, or kills it with `force`.
    pub(super) fn stop(&mut self, root: ProcRef, force: bool) -> Result<(), String> {
        self.begin_stop(root, force, None)
    }

    /// Stops the server, then reruns its launch command in the folder it
    /// started from, with its original environment when the platform can
    /// read it. The new server runs detached, with output in a log file.
    pub(super) fn restart(&mut self, port: u16, root: ProcRef) -> Result<(), String> {
        let table = self.table()?;
        let root_info = table.live(root).ok_or_else(|| gone(root))?;
        let members = table.tree(root_info);
        let launcher = self.launcher(&members);
        let details = self.details(launcher);
        let command = details
            .as_ref()
            .and_then(|d| command::shell_command(&d.args))
            .ok_or_else(|| format!("can't read the command that started :{port}"))?;
        let listener = self
            .platform
            .listeners()
            .unwrap_or_default()
            .into_iter()
            .find(|l| l.port == port)
            .and_then(|l| table.get(l.pid));
        let dir = details
            .and_then(|d| d.cwd.clone())
            .or_else(|| self.cwd(listener.unwrap_or(root_info), root_info))
            .filter(|dir| Path::new(dir).is_dir())
            .ok_or_else(|| format!("the folder :{port} started from is gone"))?;
        let environment = self
            .platform
            .environment(launcher.proc.pid)
            .filter(|env| !env.is_empty());
        let relaunch = Relaunch {
            port,
            command,
            dir,
            environment,
        };
        self.begin_stop(root, false, Some(relaunch))
    }

    /// Kills what outlived its grace period, and relaunches a restarted
    /// server once its old tree is gone and its port is free.
    pub(super) fn advance(&mut self, table: &Table, listeners: &[Listener], now: u64) {
        for mut pending in std::mem::take(&mut self.pending) {
            let alive: Vec<ProcRef> = pending
                .targets
                .iter()
                .copied()
                .filter(|t| table.live(*t).is_some())
                .collect();
            if !alive.is_empty() && pending.kill_at.is_some_and(|at| now >= at) {
                // A refusal here was already reported by the terminate.
                let _ = self.signal(&alive, true);
                pending.kill_at = None;
            }
            let port_taken = |r: &Relaunch| listeners.iter().any(|l| l.port == r.port);
            let done = alive.is_empty() && !pending.relaunch.as_ref().is_some_and(port_taken);
            if !done && now < pending.give_up_at {
                self.pending.push(pending);
            } else if let Some(r) = pending.relaunch {
                if let Err(e) = spawn(&r.command, r.environment, &r.dir, r.port) {
                    eprintln!(
                        "ppm: couldn't restart :{} with `{}`: {e}",
                        r.port, r.command
                    );
                }
            }
        }
    }

    fn begin_stop(
        &mut self,
        root: ProcRef,
        force: bool,
        relaunch: Option<Relaunch>,
    ) -> Result<(), String> {
        let targets = targets(&self.table()?, root)?;
        self.signal(&targets, force)?;
        let now = self.platform.now_ms();
        self.pending.push(Pending {
            targets,
            kill_at: (!force).then_some(now + GRACE_MS),
            give_up_at: now + GIVE_UP_MS,
            relaunch,
        });
        Ok(())
    }

    fn table(&self) -> Result<Table, String> {
        let processes = self.platform.processes().map_err(|e| e.to_string())?;
        Ok(Table::new(processes))
    }

    /// Signals each target the platform didn't interrupt. One that exited or
    /// was replaced meanwhile is skipped; a refusal (another user's process)
    /// is an error.
    fn signal(&self, targets: &[ProcRef], force: bool) -> Result<(), String> {
        let interrupted = match force {
            true => Vec::new(),
            false => self.platform.interrupt(targets),
        };
        for target in targets.iter().filter(|t| !interrupted.contains(t)) {
            match self.platform.signal(*target, force) {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    return Err(format!("not allowed to stop pid {}: {e}", target.pid));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// The live tree under `root`, deepest first. Never includes init or ppm.
fn targets(table: &Table, root: ProcRef) -> Result<Vec<ProcRef>, String> {
    let root = table.live(root).ok_or_else(|| gone(root))?;
    let own_pid = std::process::id();
    let mut members = table.tree(root);
    members.retain(|(p, _)| p.proc.pid > 1 && p.proc.pid != own_pid);
    members.sort_by_key(|(_, depth)| std::cmp::Reverse(*depth));
    Ok(members.into_iter().map(|(p, _)| p.proc).collect())
}

fn gone(root: ProcRef) -> String {
    format!(
        "pid {} has exited or now belongs to another process",
        root.pid
    )
}

fn spawn(
    command: &str,
    environment: Option<Vec<(String, String)>>,
    dir: &str,
    port: u16,
) -> io::Result<()> {
    let logs = std::env::temp_dir().join("port-process-manager");
    std::fs::create_dir_all(&logs)?;
    let log = File::create(logs.join(format!("port-{port}.log")))?;
    let mut shell = shell(command);
    shell
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if let Some(environment) = environment {
        shell.env_clear().envs(environment);
    }
    let mut child = shell.spawn()?;
    // Reap it when it exits, so it never lingers as a zombie of ppm.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(unix)]
fn shell(command: &str) -> Command {
    use std::os::unix::process::CommandExt;
    let mut shell = Command::new("/bin/sh");
    // Its own process group, so signals meant for ppm don't reach it.
    shell.arg("-c").arg(command).process_group(0);
    shell
}

#[cfg(windows)]
fn shell(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    // Its own hidden console, so Ctrl+C meant for ppm doesn't reach it, and
    // a stop can send it one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut shell = Command::new("cmd");
    shell
        .raw_arg(format!("/d /s /c \"{command}\""))
        .creation_flags(CREATE_NO_WINDOW);
    shell
}
