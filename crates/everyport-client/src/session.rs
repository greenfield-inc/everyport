//! Keeps one connection alive: runs a session, reports what happens, and
//! starts a new session with backoff when it ends.

use crate::remote;
use crate::Connection;
use everyport_core::protocol::{Call, Event, Os, Request, PROTOCOL_VERSION};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(30);

/// What a connection reports, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// A session is starting.
    Connecting,
    /// A `hello`, `snapshot` or `alert` from everyport. Results go to [`Client::call`].
    Event(Event),
    /// The session ended. The next one starts after `retry_in`.
    Disconnected { error: String, retry_in: Duration },
}

pub(crate) type Reply = oneshot::Sender<Result<(), String>>;

/// Sends requests on a connection. The connection stops when the last clone
/// is dropped.
#[derive(Clone)]
pub struct Client {
    calls: mpsc::UnboundedSender<(Call, Reply)>,
}

impl Client {
    /// Sends a request and waits for its result. `Err` carries everyport's error
    /// message, or says the connection is down.
    pub async fn call(&self, call: Call) -> Result<(), String> {
        let (reply, result) = oneshot::channel();
        self.calls
            .send((call, reply))
            .map_err(|_| "The connection is closed.".to_string())?;
        result
            .await
            .unwrap_or_else(|_| Err("The connection dropped before everyport answered.".into()))
    }
}

/// Starts a connection. Call it inside a Tokio runtime.
pub fn connect(connection: Connection) -> (Client, mpsc::UnboundedReceiver<Update>) {
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let (updates, updates_rx) = mpsc::unbounded_channel();
    tokio::spawn(supervise(connection, calls, updates));
    (Client { calls: calls_tx }, updates_rx)
}

async fn supervise(
    connection: Connection,
    mut calls: mpsc::UnboundedReceiver<(Call, Reply)>,
    updates: mpsc::UnboundedSender<Update>,
) {
    let mut retry_in = FIRST_RETRY;
    loop {
        if updates.send(Update::Connecting).is_err() {
            return;
        }
        let mut session = Session {
            calls: &mut calls,
            updates: &updates,
            greeted: false,
        };
        let ended = match &connection {
            Connection::Sidecar { path } => {
                let path = path.to_string_lossy();
                // Without a prefix nothing is quoted, so the OS doesn't matter.
                session
                    .stdio(remote::command(&[], Os::Linux, &[&path, "stdio"]))
                    .await
            }
            Connection::Command {
                argv_prefix,
                everyport_path,
            } => {
                let os = remote::os_of(everyport_path);
                session
                    .stdio(remote::command(argv_prefix, os, &[everyport_path, "stdio"]))
                    .await
            }
            Connection::Http { url, token } => crate::http::run(url, &token.0, &mut session).await,
        };
        let Ended::Error(error) = ended else { return };
        if session.greeted {
            retry_in = FIRST_RETRY;
        }
        if updates
            .send(Update::Disconnected { error, retry_in })
            .is_err()
        {
            return;
        }
        let wait = tokio::time::sleep(retry_in);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                call = calls.recv() => match call {
                    Some((_, reply)) => { let _ = reply.send(Err("Not connected.".into())); }
                    None => return,
                },
            }
        }
        retry_in = (retry_in * 2).min(LAST_RETRY);
    }
}

pub(crate) enum Ended {
    /// The client or the update receiver was dropped.
    Stopped,
    Error(String),
}

pub(crate) struct Session<'a> {
    pub(crate) calls: &'a mut mpsc::UnboundedReceiver<(Call, Reply)>,
    updates: &'a mpsc::UnboundedSender<Update>,
    greeted: bool,
}

impl Session<'_> {
    /// Passes an event on. `Some` ends the session.
    pub(crate) fn event(&mut self, event: Event) -> Option<Ended> {
        if let Event::Hello(hello) = &event {
            if hello.protocol != PROTOCOL_VERSION {
                return Some(Ended::Error(format!(
                    "This machine runs everyport {}, which speaks protocol {}. The app speaks protocol {PROTOCOL_VERSION}. Update everyport on the machine.",
                    hello.everyport_version, hello.protocol
                )));
            }
            self.greeted = true;
        }
        self.updates
            .send(Update::Event(event))
            .err()
            .map(|_| Ended::Stopped)
    }

    /// Runs `everyport stdio` and trades JSON lines with it until it exits.
    async fn stdio(&mut self, mut command: tokio::process::Command) -> Ended {
        let program = command
            .as_std()
            .get_program()
            .to_string_lossy()
            .into_owned();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => return Ended::Error(format!("Couldn't run {program}: {e}")),
        };
        let (Some(mut stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Ended::Error(format!("Couldn't talk to {program}."));
        };
        let stderr = tokio::spawn(async move {
            let mut text = Vec::new();
            let _ = stderr.read_to_end(&mut text).await;
            text
        });
        let mut lines = BufReader::new(stdout).lines();
        let mut pending: HashMap<u64, Reply> = HashMap::new();
        let mut next_id = 1;
        let ended = loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => match serde_json::from_str::<Event>(&line) {
                        Ok(Event::Result(result)) => {
                            if let Some(reply) = pending.remove(&result.id) {
                                let _ = reply.send(result.error.map_or(Ok(()), Err));
                            }
                        }
                        Ok(event) => if let Some(ended) = self.event(event) { break ended },
                        // Not protocol, such as a login banner printed by a shell profile.
                        Err(_) => {}
                    },
                    Ok(None) => break Ended::Error(String::new()),
                    Err(e) => break Ended::Error(e.to_string()),
                },
                call = self.calls.recv() => {
                    let Some((call, reply)) = call else { break Ended::Stopped };
                    let mut line = serde_json::to_string(&Request { id: next_id, call }).unwrap_or_default();
                    line.push('\n');
                    if let Err(e) = stdin.write_all(line.as_bytes()).await {
                        let _ = reply.send(Err(format!("Couldn't send the request: {e}")));
                        break Ended::Error(e.to_string());
                    }
                    pending.insert(next_id, reply);
                    next_id += 1;
                }
            }
        };
        drop(stdin);
        let status = match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
            Ok(Ok(status)) => Some(status),
            _ => {
                let _ = child.kill().await;
                None
            }
        };
        let Ended::Error(error) = ended else {
            return ended;
        };
        if !error.is_empty() {
            return Ended::Error(error);
        }
        // A grandchild can hold stderr open after the child exits.
        let stderr = match tokio::time::timeout(Duration::from_secs(1), stderr).await {
            Ok(Ok(text)) => text,
            _ => Vec::new(),
        };
        let stderr = String::from_utf8_lossy(&stderr);
        let stderr = stderr.trim();
        Ended::Error(match status {
            _ if !stderr.is_empty() => last_chars(stderr, 1000).to_string(),
            Some(status) => format!("{program} exited ({status})."),
            None => format!("{program} closed its output."),
        })
    }
}

fn last_chars(text: &str, n: usize) -> &str {
    let start = text.char_indices().rev().nth(n).map_or(0, |(i, _)| i);
    &text[start..]
}
