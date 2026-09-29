//! The machine a command runs against: this one, scanned in-process, or
//! another one running `ppm stdio` through ppm-client. Commands and the
//! terminal UI read the same events from either.

use crate::commands;
use crate::hub::Hub;
use anyhow::{bail, Context};
use ppm_client::forward::{self, Forward};
use ppm_client::install::{self, Probe};
use ppm_client::machines::{self, Via};
use ppm_client::{discover, Client, Connection, Update};
use ppm_core::protocol::{Call, Event, Request, RequestResult};
use std::io::{self, IsTerminal};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Runs connections and forwards. Only other machines start it.
pub static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime")
});

/// What a machine reports.
pub enum Feed {
    Event(Event),
    /// The connection dropped. ppm-client reconnects on its own.
    Lost(String),
}

/// Whether to install `ppm` on a machine that lacks it.
#[derive(Clone, Copy)]
pub enum Install {
    /// `--yes`: install or update without asking.
    Yes,
    /// Ask in the terminal. Without one, fail when ppm is missing.
    Ask,
    /// Never install. For the TUI's machine switcher, which can't ask.
    No,
}

pub struct Machine {
    /// The name given to `--on`, or `None` for this machine.
    pub name: Option<String>,
    feed: Receiver<Feed>,
    link: Link,
    next_id: u64,
}

enum Link {
    Local {
        hub: Hub,
        events: Sender<Event>,
    },
    Remote {
        client: Client,
        connection: Connection,
        results: Sender<Feed>,
        forwards: Vec<(u16, Forward)>,
    },
}

impl Machine {
    pub fn local() -> Self {
        let hub = Hub::start(commands::engine());
        let (events, rx) = mpsc::channel();
        hub.subscribe(events.clone());
        let (tx, feed) = mpsc::channel();
        thread::spawn(move || {
            for event in rx {
                if tx.send(Feed::Event(event)).is_err() {
                    return;
                }
            }
        });
        Self {
            name: None,
            feed,
            link: Link::Local { hub, events },
            next_id: 1,
        }
    }

    /// Connects to a saved or discovered machine, installing ppm there first
    /// when `install` allows.
    pub fn remote(name: &str, install: Install) -> Result<Self, String> {
        let connection = RUNTIME
            .block_on(connection(name, install))
            .map_err(|e| format!("{e:#}"))?;
        let _runtime = RUNTIME.enter();
        let (client, mut updates) = ppm_client::connect(connection.clone());
        let (tx, feed) = mpsc::channel();
        let results = tx.clone();
        RUNTIME.spawn(async move {
            while let Some(update) = updates.recv().await {
                let feed = match update {
                    Update::Connecting => continue,
                    Update::Event(event) => Feed::Event(event),
                    Update::Disconnected { error, .. } => Feed::Lost(error),
                };
                if tx.send(feed).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            name: Some(name.to_string()),
            feed,
            link: Link::Remote {
                client,
                connection,
                results,
                forwards: Vec::new(),
            },
            next_id: 1,
        })
    }

    pub fn is_local(&self) -> bool {
        self.name.is_none()
    }

    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or("this machine")
    }

    /// Sends a request. Its `result` arrives in the feed with the returned id.
    pub fn call(&mut self, call: Call) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        match &self.link {
            Link::Local { hub, events } => hub.call(Request { id, call }, events.clone()),
            Link::Remote {
                client, results, ..
            } => {
                let (client, results) = (client.clone(), results.clone());
                RUNTIME.spawn(async move {
                    let error = client.call(call).await.err();
                    let _ = results.send(Feed::Event(Event::Result(RequestResult { id, error })));
                });
            }
        }
        id
    }

    pub fn recv(&self) -> Option<Feed> {
        self.feed.recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Feed, RecvTimeoutError> {
        self.feed.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Option<Feed> {
        self.feed.try_recv().ok()
    }

    /// A URL that opens the server on `port` here. For another machine it
    /// forwards the port, and the forward lasts as long as this `Machine`.
    pub fn url(&mut self, port: u16) -> Result<String, String> {
        let Link::Remote {
            connection,
            forwards,
            ..
        } = &mut self.link
        else {
            return Ok(format!("http://localhost:{port}"));
        };
        if let Some((_, forward)) = forwards.iter().find(|(p, _)| *p == port) {
            return Ok(forward.url.clone());
        }
        let forward = RUNTIME
            .block_on(forward::forward(connection, port))
            .map_err(|e| format!("{e:#}"))?;
        let url = forward.url.clone();
        forwards.push((port, forward));
        Ok(url)
    }

    /// True while a URL from [`Machine::url`] needs this process to stay up.
    pub fn is_forwarding(&self) -> bool {
        matches!(&self.link, Link::Remote { forwards, .. } if forwards.iter().any(|(_, f)| f.is_tunnel()))
    }

    /// Ends the connection. On this machine, first lets the scanner finish
    /// any stop or restart, so it isn't lost when ppm exits.
    pub fn close(self) {
        let Self { feed, link, .. } = self;
        let local = matches!(link, Link::Local { .. });
        drop(link);
        if local {
            for _ in feed {}
        }
    }
}

/// How to reach `name`, from `machines.toml` first, then the machines
/// `ppm remote list` discovers.
async fn connection(name: &str, install: Install) -> anyhow::Result<Connection> {
    let prefix = match find(name).await? {
        Via::Url { url, token } => return Ok(Connection::Http { url, token }),
        Via::Command { command } => command,
    };
    let probe = install::probe(&prefix)
        .await
        .with_context(|| format!("Couldn't reach {name}"))?;
    let ppm_path = if probe.up_to_date() {
        probe.ppm_path
    } else if should_install(name, &probe, install)? {
        eprintln!("Installing ppm {} on {name}…", install::VERSION);
        install::install(&prefix, &probe).await?;
        eprintln!("Installed ppm in {} on {name}.", probe.install_path);
        probe.install_path
    } else {
        probe.ppm_path
    };
    Ok(Connection::Command {
        argv_prefix: prefix,
        ppm_path,
    })
}

/// Asks before installing or updating. An older ppm that isn't updated still
/// runs, and ppm-client reports a protocol mismatch if there is one.
fn should_install(name: &str, probe: &Probe, install: Install) -> anyhow::Result<bool> {
    let missing = probe.installed.is_none();
    match install {
        Install::Yes => Ok(true),
        Install::No if missing => {
            bail!("ppm isn't installed on {name}. Run `ppm --on {name}` to install it.")
        }
        Install::Ask if io::stdin().is_terminal() => {
            let question = match &probe.installed {
                None => format!("Install ppm on {name}?"),
                Some(version) => format!(
                    "Update ppm on {name} from {version} to {}?",
                    install::VERSION
                ),
            };
            let yes = commands::confirm(&question)?;
            if missing && !yes {
                bail!("ppm isn't installed on {name}.");
            }
            Ok(yes)
        }
        Install::Ask if missing => {
            bail!("ppm isn't installed on {name}. Run again with --yes to install it.")
        }
        _ => Ok(false),
    }
}

async fn find(name: &str) -> anyhow::Result<Via> {
    let saved = machines::load(&machines_path()?)?;
    if let Some(machine) = saved.into_iter().find(|m| m.name == name) {
        return Ok(machine.via);
    }
    discover::all()
        .await
        .into_iter()
        .find(|found| found.machine.name == name)
        .map(|found| found.machine.via)
        .with_context(|| {
            format!("No machine named {name}. Add it with `ppm remote add {name} -- ssh {name}`, or see `ppm remote list`.")
        })
}

pub fn machines_path() -> anyhow::Result<std::path::PathBuf> {
    machines::path().context("No config folder for this user")
}
