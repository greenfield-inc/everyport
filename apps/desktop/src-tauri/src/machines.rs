//! The machines the app watches and their latest state: this computer through
//! the bundled `ppm` sidecar, the machines in `machines.toml`, WSL distros,
//! and the ssh and Pane hosts discovery finds, which connect once picked.
//! Every machine runs `ppm stdio` through ppm-client, on one code path.
//! Snapshots always update the tray, but reach a page only while it shows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use ppm_client::discover::{self, Found, Source};
use ppm_client::forward::{self, Forward};
use ppm_client::install::{self, Probe};
use ppm_client::machines::{self as saved, Via};
use ppm_client::protocol::{AgentSession, Call, Event, HostInfo, Server, ServerStatus, Snapshot};
use ppm_client::{Client, Connection, Update};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{notify, popover, settings, tray};

pub const LOCAL: &str = "local";
const SNOOZE: Duration = Duration::from_secs(3600);
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(30);
/// Run numbers start at 1, so a new entry's 0 matches no task.
static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

/// `Machine` in @ppm/protocol.
#[derive(Clone, Serialize)]
pub struct Machine {
    pub id: String,
    pub label: String,
    pub host: Option<HostInfo>,
    pub state: MachineState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What installing ppm would do, while the app asks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<InstallOffer>,
    pub snapshot: Option<Snapshot>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    /// Discovered, and connects when the user picks it.
    Available,
    Connecting,
    /// ppm isn't on the machine; the app asks before installing it.
    Install,
    Installing,
    Connected,
    Error,
}

#[derive(Clone, Serialize)]
pub struct InstallOffer {
    pub version: String,
    pub path: String,
}

/// A machine and how the app reaches it.
struct Entry {
    machine: Machine,
    /// `None` for this computer.
    via: Option<Via>,
    /// The distro, for a WSL machine.
    distro: Option<String>,
    /// This run's number, unique across all machines, so tasks for an older
    /// run, or for a removed machine of the same name, stop.
    run: u64,
    connection: Option<Connection>,
    client: Option<Client>,
    /// The probe that found no ppm, kept for the install the user confirms.
    probe: Option<Probe>,
    /// This computer's snapshot as ppm sent it, before WSL relays are removed.
    raw: Option<Snapshot>,
    /// Open forwards by remote port. They end with the run, since a new run
    /// may reach another host, and dropping one closes its tunnel.
    forwards: HashMap<u16, Forward>,
}

impl Entry {
    fn new(id: &str, label: &str, via: Option<Via>, distro: Option<String>) -> Self {
        Self {
            machine: Machine {
                id: id.into(),
                label: label.into(),
                host: None,
                state: MachineState::Available,
                error: None,
                install: None,
                snapshot: None,
            },
            via,
            distro,
            run: 0,
            connection: None,
            client: None,
            probe: None,
            raw: None,
            forwards: HashMap::new(),
        }
    }
}

pub struct Machines {
    entries: Vec<Entry>,
    snoozed: HashMap<(String, u16), Instant>,
    /// `machines.toml`'s modification time when the list was last read.
    read_at: Option<SystemTime>,
}

fn machines(app: &AppHandle) -> std::sync::MutexGuard<'_, Machines> {
    app.state::<Mutex<Machines>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn file() -> Option<PathBuf> {
    saved::path()
}

fn modified() -> Option<SystemTime> {
    std::fs::metadata(file()?).ok()?.modified().ok()
}

/// Connects to this computer through the sidecar next to the app binary,
/// then lists and connects the other machines.
pub fn start(app: &AppHandle) {
    let path = std::env::current_exe()
        .expect("the app knows its own path")
        .with_file_name(format!("ppm-sidecar{}", std::env::consts::EXE_SUFFIX));
    app.manage(Mutex::new(Machines {
        entries: vec![Entry::new(LOCAL, "This computer", None, None)],
        snoozed: HashMap::new(),
        read_at: None,
    }));
    let run = begin(app, LOCAL).expect("this computer is always listed");
    connect(app, LOCAL, run, Connection::Sidecar { path });
    reload(app);
}

/// A machine in the list, and whether it connects without being picked.
#[derive(Debug, PartialEq)]
struct Listed {
    machine: saved::Machine,
    auto: bool,
    distro: Option<String>,
}

/// Saved machines first, then discovered ones that aren't saved already
/// (by name or connection). Saved machines and WSL distros connect on their
/// own; ssh and Pane hosts wait until the user picks one.
fn roster(saved: Vec<saved::Machine>, found: Vec<Found>) -> Vec<Listed> {
    let distro = |via: &Via, found: &[Found]| {
        found
            .iter()
            .find(|f| f.source == Source::Wsl && f.machine.via == *via)
            .map(|f| f.machine.name.clone())
    };
    let mut list: Vec<Listed> = saved
        .into_iter()
        .map(|machine| Listed {
            distro: distro(&machine.via, &found),
            machine,
            auto: true,
        })
        .collect();
    for f in found {
        let known = list
            .iter()
            .any(|l| l.machine.name == f.machine.name || l.machine.via == f.machine.via);
        if !known {
            let wsl = f.source == Source::Wsl;
            list.push(Listed {
                distro: wsl.then(|| f.machine.name.clone()),
                machine: f.machine,
                auto: wsl,
            });
        }
    }
    list
}

/// Reads `machines.toml` and discovery again, and connects and disconnects to
/// match. Machines whose connection didn't change keep running.
pub fn reload(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let read_at = modified();
        let listed = match file().map(|path| saved::load(&path)) {
            Some(Ok(list)) => list,
            Some(Err(error)) => {
                eprintln!("machines: {error:#}");
                Vec::new()
            }
            None => Vec::new(),
        };
        let listed = roster(listed, discover::all().await);
        let starts = {
            let mut all = machines(&app);
            all.read_at = read_at;
            let mut old = std::mem::take(&mut all.entries);
            let mut starts = Vec::new();
            let mut entries = vec![take(&mut old, LOCAL).expect("this computer is always listed")];
            for l in listed {
                let name = l.machine.name.clone();
                if name == LOCAL {
                    eprintln!(
                        "machines: skipping a machine named {LOCAL}, which is this computer's id"
                    );
                    continue;
                }
                let kept = take(&mut old, &name)
                    .filter(|e| e.via.as_ref() == Some(&l.machine.via) && e.distro == l.distro);
                let entry =
                    kept.unwrap_or_else(|| Entry::new(&name, &name, Some(l.machine.via), l.distro));
                // New, or discovered before and saved since.
                if l.auto && entry.machine.state == MachineState::Available {
                    starts.push(name);
                }
                entries.push(entry);
            }
            // Removed machines disconnect as their clients and forwards drop;
            // their tasks stop.
            all.entries = entries;
            starts
        };
        for id in starts {
            run(&app, &id);
        }
        changed(&app);
    });
}

fn take(entries: &mut Vec<Entry>, id: &str) -> Option<Entry> {
    let index = entries.iter().position(|e| e.machine.id == id)?;
    Some(entries.remove(index))
}

/// Reloads when `machines.toml` changed since it was read, such as after
/// `ppm remote add`. A single stat, done when the popover opens.
pub fn reload_if_changed(app: &AppHandle) {
    if machines(app).read_at != modified() {
        reload(app);
    }
}

/// Starts a new run for a machine, ending any older one. Returns its number.
fn begin(app: &AppHandle, id: &str) -> Option<u64> {
    let mut all = machines(app);
    let entry = all.entries.iter_mut().find(|e| e.machine.id == id)?;
    entry.run = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
    entry.client = None;
    entry.connection = None;
    entry.forwards.clear();
    entry.probe = None;
    entry.machine.state = MachineState::Connecting;
    entry.machine.error = None;
    entry.machine.install = None;
    Some(entry.run)
}

/// Changes a machine if `run` is still its current run. False when it isn't.
fn update(app: &AppHandle, id: &str, run: u64, f: impl FnOnce(&mut Entry)) -> bool {
    let current = {
        let mut all = machines(app);
        match all
            .entries
            .iter_mut()
            .find(|e| e.machine.id == id && e.run == run)
        {
            Some(entry) => {
                f(entry);
                true
            }
            None => false,
        }
    };
    if current {
        changed(app);
    }
    current
}

/// What to do after probing a machine.
#[derive(Debug, PartialEq)]
enum Plan {
    /// ppm isn't there: ask the user.
    Ask,
    /// ppm at the install path is older than the app: update it, as the
    /// README promises. A newer one is used as it is.
    Update,
    Connect,
}

fn plan(probe: &Probe) -> Plan {
    match &probe.installed {
        None => Plan::Ask,
        Some(v) if probe.ppm_path == probe.install_path && older(v, install::VERSION) => {
            Plan::Update
        }
        Some(_) => Plan::Connect,
    }
}

/// True when version `a` is older than `b`, comparing `major.minor.patch`
/// as numbers. Versions that don't parse are never older, so the app
/// doesn't replace a ppm it can't place.
fn older(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Option<Vec<u64>> {
        let core = v.split(['-', '+']).next()?;
        let parts: Option<Vec<u64>> = core.split('.').map(|n| n.parse().ok()).collect();
        parts.filter(|p| p.len() == 3)
    };
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a < b)
}

/// ppm-client ends a session with this when the machine's ppm speaks
/// another protocol version, before any hello reaches the app.
fn incompatible(error: &str) -> bool {
    error.contains("which speaks protocol")
}

/// Connects a remote machine: probes it, updates or asks to install ppm, then
/// runs `ppm stdio` there.
fn run(app: &AppHandle, id: &str) {
    let Some(run) = begin(app, id) else { return };
    let via = machines(app)
        .entries
        .iter()
        .find(|e| e.machine.id == id)
        .and_then(|e| e.via.clone());
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let prefix = match via {
            Some(Via::Command { command }) => command,
            Some(Via::Url { url, token }) => {
                return connect(&app, &id, run, Connection::Http { url, token })
            }
            None => return,
        };
        let mut retry_in = FIRST_RETRY;
        let probe = loop {
            match install::probe(&prefix).await {
                Ok(probe) => break probe,
                Err(error) => {
                    let message = format!("{error:#}. Retrying in {} s.", retry_in.as_secs());
                    eprintln!("machine {id}: {message}");
                    if !update(&app, &id, run, |e| {
                        e.machine.state = MachineState::Error;
                        e.machine.error = Some(message);
                    }) {
                        return;
                    }
                    tokio::time::sleep(retry_in).await;
                    retry_in = (retry_in * 2).min(LAST_RETRY);
                }
            }
        };
        match plan(&probe) {
            Plan::Ask => {
                eprintln!("machine {id}: ppm isn't installed, asking");
                update(&app, &id, run, |e| {
                    e.machine.state = MachineState::Install;
                    e.machine.error = None;
                    e.machine.install = Some(InstallOffer {
                        version: install::VERSION.into(),
                        path: probe.install_path.clone(),
                    });
                    e.probe = Some(probe);
                });
            }
            Plan::Update => install_and_connect(&app, &id, run, prefix, probe).await,
            Plan::Connect => {
                let connection = Connection::Command {
                    argv_prefix: prefix,
                    ppm_path: probe.ppm_path.clone(),
                };
                // Kept in case its protocol is incompatible, to ask then.
                update(&app, &id, run, |e| e.probe = Some(probe));
                connect(&app, &id, run, connection);
            }
        }
    });
}

async fn install_and_connect(
    app: &AppHandle,
    id: &str,
    run: u64,
    prefix: Vec<String>,
    probe: Probe,
) {
    eprintln!(
        "machine {id}: installing ppm {} to {}",
        install::VERSION,
        probe.install_path
    );
    if !update(app, id, run, |e| {
        e.machine.state = MachineState::Installing;
        e.machine.error = None;
        e.machine.install = None;
    }) {
        return;
    }
    match install::install(&prefix, &probe).await {
        Ok(()) => {
            let connection = Connection::Command {
                argv_prefix: prefix,
                ppm_path: probe.install_path,
            };
            connect(app, id, run, connection);
        }
        Err(error) => {
            let message = format!("Couldn't install ppm: {error:#}");
            eprintln!("machine {id}: {message}");
            // Back to the question, with the reason, so the user can try again.
            update(app, id, run, |e| {
                e.machine.state = MachineState::Install;
                e.machine.error = Some(message);
                e.machine.install = Some(InstallOffer {
                    version: install::VERSION.into(),
                    path: probe.install_path.clone(),
                });
                e.probe = Some(probe);
            });
        }
    }
}

/// Runs `ppm stdio` over `connection` for the machine's run `run`, and
/// applies what it reports until that run ends.
fn connect(app: &AppHandle, id: &str, run: u64, connection: Connection) {
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        // ppm_client::connect spawns onto the current Tokio runtime.
        let (client, mut updates) = ppm_client::connect(connection.clone());
        if !update(&app, &id, run, |e| {
            e.client = Some(client);
            e.connection = Some(connection);
        }) {
            return;
        }
        while let Some(update) = updates.recv().await {
            if !apply(&app, &id, run, update) {
                return;
            }
        }
    });
}

/// Applies an update to the machine's current run. False once the run ended.
fn apply(app: &AppHandle, id: &str, run: u64, update: Update) -> bool {
    let mut alert = None;
    let connected = matches!(update, Update::Event(Event::Hello(_)));
    {
        let mut all = machines(app);
        let Some(entry) = all
            .entries
            .iter_mut()
            .find(|e| e.machine.id == id && e.run == run)
        else {
            return false;
        };
        let machine = &mut entry.machine;
        match update {
            Update::Connecting => machine.state = MachineState::Connecting,
            Update::Disconnected { error, .. } if incompatible(&error) && entry.probe.is_some() => {
                // A ppm this app can't talk to: stop retrying and ask to install ours.
                eprintln!(
                    "machine {id}: {error} Asking to install ppm {}.",
                    install::VERSION
                );
                let path = entry.probe.as_ref().map(|p| p.install_path.clone());
                entry.client = None;
                entry.connection = None;
                machine.state = MachineState::Install;
                machine.error = Some(error);
                machine.install = path.map(|path| InstallOffer {
                    version: install::VERSION.into(),
                    path,
                });
                drop(all);
                changed(app);
                return false;
            }
            Update::Disconnected { error, retry_in } => {
                // Its servers are unknown now; the page shows the error instead.
                eprintln!("machine {id}: {error}");
                // Its tunnels may have died with the connection.
                entry.forwards.clear();
                entry.raw = None;
                machine.snapshot = None;
                machine.state = MachineState::Error;
                machine.error = Some(format!("{error}. Retrying in {} s.", retry_in.as_secs()));
            }
            Update::Event(Event::Hello(hello)) => {
                eprintln!(
                    "machine {id}: connected to {} ({:?}, ppm {})",
                    hello.host.hostname, hello.host.os, hello.ppm_version
                );
                // Other machines keep the name the user knows them by.
                if id == LOCAL {
                    machine.label = hello.host.hostname.clone();
                }
                machine.host = Some(hello.host);
                machine.state = MachineState::Connected;
                machine.error = None;
            }
            Update::Event(Event::Snapshot(snapshot)) => {
                let ports: Vec<u16> = snapshot.servers.iter().map(|s| s.port).collect();
                if id == LOCAL {
                    entry.raw = Some(snapshot);
                } else {
                    machine.snapshot = Some(snapshot);
                }
                // A tunnel to a server that stopped has nothing left to carry.
                entry.forwards.retain(|port, _| ports.contains(port));
                all.show_local();
            }
            Update::Event(Event::Alert(event)) => {
                let key = (id.to_string(), event.port);
                let snoozed = all
                    .snoozed
                    .get(&key)
                    .is_some_and(|until| *until > Instant::now());
                let server = all.server(id, event.port).cloned();
                if let (false, Some(server)) = (snoozed, server) {
                    alert = Some((server, event));
                }
            }
            Update::Event(Event::Result(_)) => return true,
        }
    }
    changed(app);
    if connected {
        configure(app, id);
    }
    if let Some((server, event)) = alert {
        notify::show(app, id, server, event);
    }
    true
}

/// Sends the user's settings to a machine that just connected.
fn configure(app: &AppHandle, id: &str) {
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let Some(change) = settings::change_for(&app, &id) else {
            return;
        };
        if let Err(error) = call(&app, &id, Call::Configure(change)).await {
            eprintln!("configure {id}: {error}");
        }
    });
}

impl Machines {
    fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.machine.id == id)
    }

    fn server(&self, id: &str, port: u16) -> Option<&Server> {
        self.entry(id)?
            .machine
            .snapshot
            .as_ref()?
            .servers
            .iter()
            .find(|s| s.port == port)
    }

    /// This computer's snapshot, without the ports WSL relays for a distro
    /// that shows them itself, or the app's own tunnels to other machines,
    /// which belong to the remote server (and stopping one would stop the
    /// app or its ssh tunnel). This computer and WSL open ports directly,
    /// with no tunnel.
    fn show_local(&mut self) {
        let tunnels: Vec<u16> = self
            .entries
            .iter()
            .filter(|e| e.machine.id != LOCAL && e.distro.is_none())
            .flat_map(|e| e.forwards.values().map(|f| f.local_port))
            .collect();
        let distros: Vec<&Snapshot> = self
            .entries
            .iter()
            .filter(|e| e.distro.is_some())
            .filter_map(|e| e.machine.snapshot.as_ref())
            .collect();
        let local = self
            .entry(LOCAL)
            .and_then(|e| e.raw.clone())
            .map(|mut raw| {
                ppm_client::wsl::dedupe(&mut raw, &distros);
                raw.servers.retain(|server| !tunnels.contains(&server.port));
                raw
            });
        if let Some(entry) = self.entries.iter_mut().find(|e| e.machine.id == LOCAL) {
            entry.machine.snapshot = local;
        }
    }
}

/// Something shown changed: redraw the tray and send the list to the pages.
fn changed(app: &AppHandle) {
    update_tray(app);
    publish(app);
}

fn update_tray(app: &AppHandle) {
    let all = machines(app);
    let servers = all
        .entries
        .iter()
        .filter_map(|e| e.machine.snapshot.as_ref())
        .flat_map(|s| &s.servers);
    let (count, attention) = servers.fold((0, false), |(n, attention), s| {
        (n + 1, attention || s.status == ServerStatus::Attention)
    });
    drop(all);
    tray::set_state(
        app,
        match (count, attention) {
            (0, _) => tray::State::Idle,
            (n, true) => tray::State::Attention(n),
            (n, false) => tray::State::Running(n),
        },
    );
}

fn list(app: &AppHandle) -> Vec<Machine> {
    machines(app)
        .entries
        .iter()
        .map(|e| e.machine.clone())
        .collect()
}

/// Sends every machine to the popover, if it's showing or still waiting for
/// its first data, and to Settings while it's open.
pub fn publish(app: &AppHandle) {
    let settings = app
        .get_webview_window(settings::LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false));
    let popover = popover::wants_data(app);
    if !(settings || popover) {
        return;
    }
    let list = list(app);
    if popover {
        let _ = app.emit_to(popover::LABEL, "machines", &list);
    }
    if settings {
        let _ = app.emit_to(settings::LABEL, "machines", &list);
    }
}

/// The server on `port`, from the latest snapshot.
pub fn find_server(app: &AppHandle, machine_id: &str, port: u16) -> Option<Server> {
    machines(app).server(machine_id, port).cloned()
}

/// The WSL distro a machine is, if it is one.
pub fn distro(app: &AppHandle, machine_id: &str) -> Option<String> {
    machines(app).entry(machine_id)?.distro.clone()
}

/// The agent session with this id, from the server it started, and that
/// server's folder.
pub fn find_session(
    app: &AppHandle,
    machine_id: &str,
    id: &str,
) -> Option<(AgentSession, Option<String>)> {
    let all = machines(app);
    all.entry(machine_id)?
        .machine
        .snapshot
        .as_ref()?
        .servers
        .iter()
        .find_map(|server| {
            let session = server.agent.as_ref().filter(|s| s.id == id)?;
            Some((session.clone(), server.cwd.clone()))
        })
}

pub fn snooze(app: &AppHandle, machine_id: &str, port: u16) {
    machines(app)
        .snoozed
        .insert((machine_id.to_string(), port), Instant::now() + SNOOZE);
}

pub async fn call(app: &AppHandle, machine_id: &str, call: Call) -> Result<(), String> {
    let client = machines(app)
        .entry(machine_id)
        .and_then(|e| e.client.clone())
        .ok_or_else(|| format!("{machine_id} isn't connected"))?;
    client.call(call).await
}

/// A URL that opens the server on `port` from this computer. Another
/// machine's port is forwarded here first, and stays forwarded while its
/// server runs.
pub async fn url(app: &AppHandle, machine_id: &str, port: u16) -> Result<String, String> {
    let (run, connection) = {
        let all = machines(app);
        let entry = all
            .entry(machine_id)
            .ok_or_else(|| format!("no machine {machine_id}"))?;
        // A tunnel that died, such as an ssh -L whose connection dropped,
        // no longer listens, and is made again.
        if let Some(forward) = entry.forwards.get(&port).filter(|f| listens(f.local_port)) {
            return Ok(forward.url.clone());
        }
        let connection = entry
            .connection
            .clone()
            .ok_or_else(|| format!("{machine_id} isn't connected"))?;
        (entry.run, connection)
    };
    let forward = forward::forward(&connection, port)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let mut all = machines(app);
    let entry = all
        .entries
        .iter_mut()
        .find(|e| e.machine.id == machine_id && e.run == run)
        .ok_or_else(|| format!("{machine_id} disconnected"))?;
    // Another open of the same port may have finished first; keep a live tunnel.
    let url = match entry.forwards.get(&port) {
        Some(other) if listens(other.local_port) => other.url.clone(),
        _ => {
            let url = forward.url.clone();
            entry.forwards.insert(port, forward);
            url
        }
    };
    eprintln!("machine {machine_id}: :{port} opens at {url}");
    all.show_local();
    drop(all);
    changed(app);
    Ok(url)
}

/// Whether something accepts connections on this computer's `port`.
fn listens(port: u16) -> bool {
    let address = (std::net::Ipv4Addr::LOCALHOST, port).into();
    std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok()
}

/// Installs ppm on a machine that is waiting for the user's yes, then connects.
pub fn install(app: &AppHandle, machine_id: &str) -> Result<(), String> {
    let (run, prefix, probe) = {
        let mut all = machines(app);
        let entry = all
            .entries
            .iter_mut()
            .find(|e| e.machine.id == machine_id)
            .ok_or_else(|| format!("no machine {machine_id}"))?;
        let asking = entry.machine.state == MachineState::Install;
        let (true, Some(Via::Command { command }), Some(probe)) =
            (asking, &entry.via, entry.probe.take())
        else {
            return Err(format!("{machine_id} isn't waiting to install ppm"));
        };
        (entry.run, command.clone(), probe)
    };
    let (app, id) = (app.clone(), machine_id.to_string());
    tauri::async_runtime::spawn(async move {
        install_and_connect(&app, &id, run, prefix, probe).await;
    });
    Ok(())
}

/// Connects a machine the user picked. A discovered one is saved to
/// `machines.toml` first, so it stays in the list.
pub fn pick(app: &AppHandle, machine_id: &str) -> Result<(), String> {
    let via = {
        let all = machines(app);
        let entry = all
            .entry(machine_id)
            .ok_or_else(|| format!("no machine {machine_id}"))?;
        if entry.machine.state != MachineState::Available {
            return Ok(());
        }
        entry
            .via
            .clone()
            .ok_or("this computer is always connected")?
    };
    if let Some(path) = file() {
        let mut list = saved::load(&path).map_err(|e| format!("{e:#}"))?;
        if !list.iter().any(|m| m.name == machine_id) {
            list.push(saved::Machine {
                name: machine_id.into(),
                via,
            });
            saved::save(&path, &list).map_err(|e| format!("{e:#}"))?;
        }
        machines(app).read_at = modified();
    }
    run(app, machine_id);
    Ok(())
}

#[tauri::command]
pub fn machines_list(app: AppHandle) -> Vec<Machine> {
    list(&app)
}

#[tauri::command]
pub async fn call_machine(app: AppHandle, machine_id: String, call: Call) -> Result<(), String> {
    self::call(&app, &machine_id, call).await
}

#[tauri::command]
pub fn connect_machine(app: AppHandle, machine_id: String) -> Result<(), String> {
    pick(&app, &machine_id)
}

#[tauri::command]
pub fn install_ppm(app: AppHandle, machine_id: String) -> Result<(), String> {
    install(&app, &machine_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppm_client::protocol::Os;

    fn machine(name: &str, command: &[&str]) -> saved::Machine {
        saved::Machine {
            name: name.into(),
            via: Via::Command {
                command: command.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn found(source: Source, machine: saved::Machine) -> Found {
        Found { machine, source }
    }

    #[test]
    fn lists_saved_machines_then_new_discoveries() {
        let saved = vec![
            machine("devbox", &["ssh", "devbox"]),
            machine("box", &["docker", "exec", "-i", "box"]),
        ];
        let found = vec![
            // Already saved under its own name, and under another name.
            found(Source::SshConfig, machine("devbox", &["ssh", "devbox"])),
            found(Source::Pane, machine("Dev box", &["ssh", "devbox"])),
            found(Source::SshConfig, machine("mini", &["ssh", "mini"])),
            found(
                Source::Wsl,
                machine("Ubuntu", &["wsl.exe", "-d", "Ubuntu", "--exec"]),
            ),
        ];
        let listed = roster(saved, found);
        let summary: Vec<(&str, bool, Option<&str>)> = listed
            .iter()
            .map(|l| (l.machine.name.as_str(), l.auto, l.distro.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                ("devbox", true, None),
                ("box", true, None),
                // The README: ssh hosts are listed, and connect once picked.
                ("mini", false, None),
                // "On Windows, each installed WSL distro is added for you."
                ("Ubuntu", true, Some("Ubuntu")),
            ]
        );
    }

    #[test]
    fn a_saved_wsl_distro_is_still_that_distro() {
        let wsl = machine("Ubuntu", &["wsl.exe", "-d", "Ubuntu", "--exec"]);
        let listed = roster(
            vec![machine(
                "ubuntu box",
                &["wsl.exe", "-d", "Ubuntu", "--exec"],
            )],
            vec![found(Source::Wsl, wsl)],
        );
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].distro.as_deref(), Some("Ubuntu"));
    }

    fn probe(ppm_path: &str, installed: Option<&str>) -> Probe {
        Probe {
            os: Os::Linux,
            target: "x86_64-unknown-linux-musl".into(),
            install_path: "/home/me/.local/bin/ppm".into(),
            ppm_path: ppm_path.into(),
            installed: installed.map(String::from),
        }
    }

    #[test]
    fn asks_before_installing_and_updates_only_its_own_install() {
        let ours = "/home/me/.local/bin/ppm";
        assert_eq!(plan(&probe(ours, None)), Plan::Ask);
        assert_eq!(plan(&probe(ours, Some("0.0.1"))), Plan::Update);
        assert_eq!(plan(&probe(ours, Some(install::VERSION))), Plan::Connect);
        // A newer one, such as from install.sh, is never downgraded.
        assert_eq!(plan(&probe(ours, Some("99.0.0"))), Plan::Connect);
        // One the user installed themselves, such as with brew, is theirs to update.
        assert_eq!(plan(&probe("ppm", Some("0.0.1"))), Plan::Connect);
    }

    #[test]
    fn compares_versions_as_numbers() {
        assert!(older("0.9.0", "0.10.0"));
        assert!(older("1.2.3", "1.3.0"));
        assert!(older("0.1.0-rc.1", "0.2.0"));
        assert!(!older("0.10.0", "0.9.0"));
        assert!(!older("0.1.0", "0.1.0"));
        assert!(!older("nightly", "0.1.0"));
    }

    #[test]
    fn recognizes_ppm_clients_protocol_mismatch() {
        // The message ppm-client's session ends with, for a ppm 0.3.0 speaking protocol 2.
        assert!(incompatible("This machine runs ppm 0.3.0, which speaks protocol 2. The app speaks protocol 1. Update ppm on the machine."));
        assert!(!incompatible("ppm exited: connection reset"));
    }
}
