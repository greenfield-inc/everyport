//! The machines the app watches and their latest state. For now that is this
//! computer, through the bundled `ppm` sidecar. Snapshots always update the
//! tray, but reach the page only while the popover is visible.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ppm_client::protocol::{Call, Event, HostInfo, Server, ServerStatus, Snapshot};
use ppm_client::{Client, Connection, Update};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{notify, popover, tray};

pub const LOCAL: &str = "local";
const SNOOZE: Duration = Duration::from_secs(3600);

/// `Machine` in @ppm/protocol.
#[derive(Clone, Serialize)]
pub struct Machine {
    pub id: String,
    pub label: String,
    pub host: Option<HostInfo>,
    pub state: MachineState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub snapshot: Option<Snapshot>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    Connecting,
    Connected,
    Error,
}

#[derive(Default)]
pub struct Machines {
    list: Vec<Machine>,
    clients: HashMap<String, Client>,
    snoozed: HashMap<(String, u16), Instant>,
}

fn machines(app: &AppHandle) -> std::sync::MutexGuard<'_, Machines> {
    app.state::<Mutex<Machines>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Connects to this computer through the sidecar next to the app binary.
pub fn start(app: &AppHandle) {
    let path = std::env::current_exe()
        .expect("the app knows its own path")
        .with_file_name(format!("ppm{}", std::env::consts::EXE_SUFFIX));
    // ppm_client::connect spawns onto the current Tokio runtime.
    let (client, mut updates) =
        tauri::async_runtime::block_on(async { ppm_client::connect(Connection::Sidecar { path }) });
    app.manage(Mutex::new(Machines {
        list: vec![Machine {
            id: LOCAL.into(),
            label: "This computer".into(),
            host: None,
            state: MachineState::Connecting,
            error: None,
            snapshot: None,
        }],
        clients: HashMap::from([(LOCAL.to_string(), client)]),
        snoozed: HashMap::new(),
    }));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(update) = updates.recv().await {
            apply(&app, LOCAL, update);
        }
    });
}

fn apply(app: &AppHandle, id: &str, update: Update) {
    let mut alert = None;
    {
        let mut all = machines(app);
        let Some(machine) = all.list.iter_mut().find(|m| m.id == id) else {
            return;
        };
        match update {
            Update::Connecting => machine.state = MachineState::Connecting,
            Update::Disconnected { error, retry_in } => {
                machine.state = MachineState::Error;
                machine.error = Some(format!("{error}. Retrying in {} s.", retry_in.as_secs()));
            }
            Update::Event(Event::Hello(hello)) => {
                machine.label = hello.host.hostname.clone();
                machine.host = Some(hello.host);
                machine.state = MachineState::Connected;
                machine.error = None;
            }
            Update::Event(Event::Snapshot(snapshot)) => machine.snapshot = Some(snapshot),
            Update::Event(Event::Alert(event)) => {
                let key = (id.to_string(), event.port);
                let snoozed = all
                    .snoozed
                    .get(&key)
                    .is_some_and(|until| *until > Instant::now());
                let machine = all.list.iter().find(|m| m.id == id);
                let server = machine.and_then(|m| server(m, event.port)).cloned();
                if let (false, Some(server)) = (snoozed, server) {
                    alert = Some((server, event));
                }
            }
            Update::Event(Event::Result(_)) => return,
        }
    }
    update_tray(app);
    publish(app);
    if let Some((server, event)) = alert {
        notify::show(app, id, server, event);
    }
}

fn server(machine: &Machine, port: u16) -> Option<&Server> {
    machine
        .snapshot
        .as_ref()?
        .servers
        .iter()
        .find(|s| s.port == port)
}

fn update_tray(app: &AppHandle) {
    let all = machines(app);
    let servers = all
        .list
        .iter()
        .filter_map(|m| m.snapshot.as_ref())
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

/// Sends every machine to the page, if it's showing or still waiting for
/// its first data.
pub fn publish(app: &AppHandle) {
    if !popover::wants_data(app) {
        return;
    }
    let list = machines(app).list.clone();
    let _ = app.emit_to(popover::LABEL, "machines", list);
}

/// The server on `port`, from the latest snapshot.
pub fn find_server(app: &AppHandle, machine_id: &str, port: u16) -> Option<Server> {
    let all = machines(app);
    let machine = all.list.iter().find(|m| m.id == machine_id)?;
    server(machine, port).cloned()
}

pub fn snooze(app: &AppHandle, machine_id: &str, port: u16) {
    machines(app)
        .snoozed
        .insert((machine_id.to_string(), port), Instant::now() + SNOOZE);
}

pub async fn call(app: &AppHandle, machine_id: &str, call: Call) -> Result<(), String> {
    let client = machines(app)
        .clients
        .get(machine_id)
        .cloned()
        .ok_or_else(|| format!("no machine {machine_id}"))?;
    client.call(call).await
}

#[tauri::command]
pub fn machines_list(app: AppHandle) -> Vec<Machine> {
    machines(&app).list.clone()
}

#[tauri::command]
pub async fn call_machine(app: AppHandle, machine_id: String, call: Call) -> Result<(), String> {
    self::call(&app, &machine_id, call).await
}
