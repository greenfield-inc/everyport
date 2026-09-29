//! One scanner shared by every consumer in this process. `stdio`, `serve`,
//! `watch` and the TUI subscribe to its events and send it calls.

use ppm_core::engine::Engine;
use ppm_core::protocol::{
    Alert, Call, Event, Hello, HostInfo, Request, RequestResult, Snapshot, PROTOCOL_VERSION,
};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// What the hub needs from the engine.
pub trait Scanner: Send + 'static {
    fn host(&self) -> HostInfo;
    fn interval(&self) -> Duration;
    fn scan(&mut self) -> (Snapshot, Vec<Alert>);
    fn call(&mut self, call: &Call) -> Result<(), String>;
}

impl Scanner for Engine {
    fn host(&self) -> HostInfo {
        Engine::host(self)
    }
    fn interval(&self) -> Duration {
        Duration::from_millis(self.config().interval_ms.max(100))
    }
    fn scan(&mut self) -> (Snapshot, Vec<Alert>) {
        Engine::scan(self)
    }
    fn call(&mut self, call: &Call) -> Result<(), String> {
        Engine::call(self, call)
    }
}

enum Message {
    Subscribe(Sender<Event>),
    Call(Request, Sender<Event>),
    Reply(Event, Sender<Event>),
}

/// A handle to the scanner thread. The thread ends when every handle is dropped.
#[derive(Clone)]
pub struct Hub {
    tx: Sender<Message>,
}

impl Hub {
    pub fn start(scanner: impl Scanner) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || run(scanner, rx));
        Self { tx }
    }

    /// Sends `hello` and the latest snapshot to `events`, then every later
    /// snapshot that differs, and every alert.
    pub fn subscribe(&self, events: Sender<Event>) {
        let _ = self.tx.send(Message::Subscribe(events));
    }

    /// Runs `request` and sends its `result` to `reply`.
    pub fn call(&self, request: Request, reply: Sender<Event>) {
        let _ = self.tx.send(Message::Call(request, reply));
    }

    /// Sends `event` to `to` after everything queued before it.
    pub fn reply(&self, event: Event, to: Sender<Event>) {
        let _ = self.tx.send(Message::Reply(event, to));
    }
}

fn run(mut scanner: impl Scanner, rx: mpsc::Receiver<Message>) {
    let hello = Event::Hello(Hello {
        protocol: PROTOCOL_VERSION,
        ppm_version: env!("CARGO_PKG_VERSION").into(),
        host: scanner.host(),
    });
    let mut subscribers: Vec<Sender<Event>> = Vec::new();
    let (mut latest, _) = scanner.scan();
    let mut next_scan = Instant::now() + scanner.interval();

    loop {
        let wait = next_scan.saturating_duration_since(Instant::now());
        let force = match rx.recv_timeout(wait) {
            Ok(Message::Subscribe(events)) => {
                if events.send(hello.clone()).is_ok()
                    && events.send(Event::Snapshot(latest.clone())).is_ok()
                {
                    subscribers.push(events);
                }
                continue;
            }
            Ok(Message::Call(request, reply)) => {
                let error = scanner.call(&request.call).err();
                let _ = reply.send(Event::Result(RequestResult {
                    id: request.id,
                    error,
                }));
                matches!(request.call, Call::Refresh)
            }
            Ok(Message::Reply(event, to)) => {
                let _ = to.send(event);
                continue;
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };

        let (snapshot, alerts) = scanner.scan();
        next_scan = Instant::now() + scanner.interval();
        let mut events: Vec<Event> = alerts.into_iter().map(Event::Alert).collect();
        if force || !same_state(&snapshot, &latest) {
            events.insert(0, Event::Snapshot(snapshot.clone()));
        }
        latest = snapshot;
        subscribers.retain(|subscriber| events.iter().all(|e| subscriber.send(e.clone()).is_ok()));
    }
}

/// Reads one `Request`. A bad one becomes the `result` error to send back,
/// with the request's id when it has one.
pub fn parse_request(json: &str) -> Result<Request, Event> {
    serde_json::from_str(json).map_err(|error| {
        let id = serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|value| value.get("id")?.as_u64())
            .unwrap_or(0);
        Event::Result(RequestResult {
            id,
            error: Some(format!("invalid request: {error}")),
        })
    })
}

/// Snapshots differ only by `taken_at` when nothing changed.
fn same_state(a: &Snapshot, b: &Snapshot) -> bool {
    a.system == b.system && a.servers == b.servers
}

/// A scanner that reports the shared fixture snapshot, with a leak alert on
/// its second scan.
#[cfg(test)]
pub mod fixture {
    use super::*;
    use ppm_core::protocol::{AlertKind, Os};

    pub struct Fixture {
        scans: u32,
    }

    pub fn hub() -> Hub {
        Hub::start(Fixture { scans: 0 })
    }

    pub fn snapshot() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../packages/protocol/fixtures/snapshot.json"
        ))
        .unwrap()
    }

    impl Scanner for Fixture {
        fn host(&self) -> HostInfo {
            HostInfo {
                hostname: "devbox".into(),
                os: Os::Linux,
                arch: "x86_64".into(),
                cores: 8,
            }
        }
        fn interval(&self) -> Duration {
            Duration::from_secs(3600)
        }
        fn scan(&mut self) -> (Snapshot, Vec<Alert>) {
            self.scans += 1;
            let alerts = if self.scans == 2 {
                vec![Alert {
                    port: 6006,
                    kind: AlertKind::Leaking,
                    memory: 2_147_483_648,
                }]
            } else {
                Vec::new()
            };
            (snapshot(), alerts)
        }
        fn call(&mut self, call: &Call) -> Result<(), String> {
            match call {
                Call::Stop { .. } => Err("pid 48198 was reused".into()),
                _ => Ok(()),
            }
        }
    }
}
