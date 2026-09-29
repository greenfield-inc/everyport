//! The machine a command runs against: this one, scanned in-process, or
//! another one running `everyport stdio` through `everyport::client`. Commands and the
//! terminal UI read the same events from either.

use crate::commands;
use crate::hub::Hub;
use anyhow::{bail, Context};
use everyport::client::forward::{self, Forward};
use everyport::client::install::{self, Probe};
use everyport::client::machines::{self, Via};
use everyport::client::{discover, Client, Connection, Update};
use everyport::protocol::{Call, Event, Request, RequestResult};
use std::io;
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
    /// The connection dropped. `everyport::client` reconnects on its own.
    Lost(String),
}

/// Whether to install `everyport` on a machine that lacks it.
#[derive(Clone, Copy)]
pub enum Install {
    /// `--yes`: install or update without asking.
    Yes,
    /// Ask in the terminal. Without one, fail when everyport is missing.
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
    pub fn local() -> io::Result<Self> {
        let hub = Hub::start(commands::engine()?);
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
        Ok(Self {
            name: None,
            feed,
            link: Link::Local { hub, events },
            next_id: 1,
        })
    }

    /// Connects to a saved or discovered machine, installing everyport there first
    /// when `install` allows.
    pub fn remote(name: &str, install: Install) -> Result<Self, String> {
        let connection = connection(name, install)?;
        let _runtime = RUNTIME.enter();
        let (client, mut updates) = everyport::client::connect(connection.clone());
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

    /// Ends the connection. On this machine, first lets the scanner finish
    /// any stop or restart, so it isn't lost when everyport exits.
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
/// `everyport remote list` discovers. Installs everyport there first when `install`
/// allows.
pub fn connection(name: &str, install: Install) -> Result<Connection, String> {
    RUNTIME
        .block_on(reach(name, install))
        .map_err(|e| format!("{e:#}"))
}

async fn reach(name: &str, install: Install) -> anyhow::Result<Connection> {
    let prefix = match find(name).await? {
        Via::Url { url, token } => return Ok(Connection::Http { url, token }),
        Via::Command { command } => command,
    };
    let probe = install::probe(&prefix)
        .await
        .with_context(|| format!("Couldn't reach {name}"))?;
    let everyport_path = if probe.up_to_date() {
        probe.everyport_path
    } else if should_install(
        name,
        &probe,
        install,
        commands::can_ask().then_some(&mut commands::confirm),
    )? {
        eprintln!("Installing everyport {} on {name}…", install::VERSION);
        install::install(&prefix, &probe).await?;
        eprintln!("Installed everyport in {} on {name}.", probe.install_path);
        probe.install_path
    } else {
        probe.everyport_path
    };
    Ok(Connection::Command {
        argv_prefix: prefix,
        everyport_path,
    })
}

/// Asks a person a yes or no question.
type Ask<'a> = &'a mut dyn FnMut(&str) -> io::Result<bool>;

/// Asks before installing or updating, through `ask` when a person can
/// answer. An older everyport that isn't updated still runs, and `everyport::client` reports
/// a protocol mismatch if there is one.
fn should_install(
    name: &str,
    probe: &Probe,
    install: Install,
    ask: Option<Ask>,
) -> anyhow::Result<bool> {
    let missing = probe.installed.is_none();
    match (install, ask) {
        (Install::Yes, _) => Ok(true),
        (Install::No, _) if missing => {
            bail!("everyport isn't installed on {name}. Run `everyport --on {name}` to install it.")
        }
        (Install::Ask, Some(ask)) => {
            let question = match &probe.installed {
                None => format!("Install everyport on {name}?"),
                Some(version) => format!(
                    "Update everyport on {name} from {version} to {}?",
                    install::VERSION
                ),
            };
            let yes = ask(&question)?;
            if missing && !yes {
                bail!("everyport isn't installed on {name}.");
            }
            Ok(yes)
        }
        (Install::Ask, None) if missing => {
            bail!("everyport isn't installed on {name}. Run again with --yes to install it.")
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
            format!("No machine named {name}. Add it with `everyport remote add {name} -- ssh {name}`, or see `everyport remote list`.")
        })
}

pub fn machines_path() -> anyhow::Result<std::path::PathBuf> {
    machines::path().context("No config folder for this user")
}

#[cfg(test)]
mod tests {
    use super::*;
    use everyport::protocol::Os;

    fn probe(installed: Option<&str>) -> Probe {
        Probe {
            os: Os::Linux,
            target: "x86_64-unknown-linux-musl".into(),
            install_path: "/home/me/.local/bin/everyport".into(),
            everyport_path: "/home/me/.local/bin/everyport".into(),
            installed: installed.map(String::from),
        }
    }

    /// Runs `should_install` with a person who answers `answer`, and returns
    /// the result and the question they saw.
    fn answered(installed: Option<&str>, answer: bool) -> (anyhow::Result<bool>, Option<String>) {
        let mut asked = None;
        let mut ask = |question: &str| {
            asked = Some(question.to_string());
            Ok(answer)
        };
        let result = should_install("box", &probe(installed), Install::Ask, Some(&mut ask));
        (result, asked)
    }

    #[test]
    fn yes_installs_without_asking() {
        let mut ask = |_: &str| -> io::Result<bool> { panic!("asked") };
        assert!(should_install("box", &probe(None), Install::Yes, Some(&mut ask)).unwrap());
        assert!(should_install("box", &probe(Some("0.0.1")), Install::Yes, None).unwrap());
    }

    #[test]
    fn a_terminal_asks_before_installing() {
        let (result, asked) = answered(None, true);
        assert!(result.unwrap());
        assert_eq!(asked.as_deref(), Some("Install everyport on box?"));

        let (result, _) = answered(None, false);
        assert_eq!(
            result.unwrap_err().to_string(),
            "everyport isn't installed on box."
        );

        let (result, asked) = answered(Some("0.0.1"), false);
        assert!(
            !result.unwrap(),
            "declining an update keeps the old everyport"
        );
        assert_eq!(
            asked,
            Some(format!(
                "Update everyport on box from 0.0.1 to {}?",
                install::VERSION
            ))
        );
    }

    #[test]
    fn without_a_terminal_it_fails_rather_than_wait() {
        let error = should_install("box", &probe(None), Install::Ask, None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "everyport isn't installed on box. Run again with --yes to install it."
        );
        // An outdated everyport still runs.
        assert!(!should_install("box", &probe(Some("0.0.1")), Install::Ask, None).unwrap());
    }

    #[test]
    fn the_machine_switcher_never_installs() {
        let error = should_install("box", &probe(None), Install::No, None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "everyport isn't installed on box. Run `everyport --on box` to install it."
        );
        assert!(!should_install("box", &probe(Some("0.0.1")), Install::No, None).unwrap());
    }
}
