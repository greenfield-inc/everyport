//! Stop and restart. Every target is checked by ProcRef, so a reused pid is
//! never signalled.

use super::tree::Table;
use super::{command, Engine};
use crate::protocol::ProcRef;
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// How long a server gets to exit after a terminate before it is killed.
const GRACE: Duration = Duration::from_secs(3);
/// How long a killed process gets to disappear.
const KILL_WAIT: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(100);
/// How long restart waits for the old server to release its port.
const PORT_WAIT: Duration = Duration::from_secs(5);

impl Engine {
    /// Terminates the tree under `root`, leaves first, and kills anything
    /// still running after the grace period. Returns once all of it is gone.
    pub(super) fn stop(&mut self, root: ProcRef, force: bool) -> Result<(), String> {
        let table = self.table()?;
        let targets = targets(&table, root)?;
        self.signal(&targets, force)?;
        let mut forced = force;
        let mut deadline = Instant::now() + if force { KILL_WAIT } else { GRACE };
        loop {
            let table = self.table()?;
            let alive: Vec<ProcRef> = targets
                .iter()
                .copied()
                .filter(|t| table.live(*t).is_some())
                .collect();
            if alive.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                if forced {
                    let pids: Vec<String> = alive.iter().map(|t| t.pid.to_string()).collect();
                    return Err(format!("still running: pid {}", pids.join(", ")));
                }
                self.signal(&alive, true)?;
                forced = true;
                deadline = Instant::now() + KILL_WAIT;
            }
            sleep(POLL);
        }
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
        let environment = self.platform.environment(launcher.proc.pid);

        self.stop(root, false)?;
        let deadline = Instant::now() + PORT_WAIT;
        while Instant::now() < deadline && self.is_listening(port) {
            sleep(POLL);
        }
        spawn(&command, environment, &dir, port)
            .map_err(|e| format!("couldn't start `{command}`: {e}"))
    }

    fn table(&self) -> Result<Table, String> {
        let processes = self.platform.processes().map_err(|e| e.to_string())?;
        Ok(Table::new(processes))
    }

    fn is_listening(&self, port: u16) -> bool {
        let listeners = self.platform.listeners().unwrap_or_default();
        listeners.iter().any(|l| l.port == port)
    }

    /// Signals each target. One that exited or was replaced meanwhile is
    /// skipped; a refusal (another user's process) is an error.
    fn signal(&self, targets: &[ProcRef], force: bool) -> Result<(), String> {
        for target in targets {
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
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut shell = Command::new("cmd");
    shell
        .raw_arg(format!("/d /s /c \"{command}\""))
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    shell
}
