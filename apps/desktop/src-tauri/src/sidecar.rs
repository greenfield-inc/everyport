//! Runs `<ppm> stdio` and streams its events, restarting it with backoff.
//! Mirrors `ppm_client::connect(Connection::Sidecar { path })` so the app
//! switches to ppm-client by changing this module's callers' import.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use ppm_client::protocol::{Call, Event, Request};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

pub enum Update {
    Connecting,
    Event(Event),
    Disconnected { error: String, retry_in: Duration },
}

type Pending = oneshot::Sender<Result<(), String>>;

pub struct Client {
    requests: mpsc::UnboundedSender<(Call, Pending)>,
}

impl Client {
    /// Sends `call` and resolves with the matching `result` event.
    pub async fn call(&self, call: Call) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.requests
            .send((call, tx))
            .map_err(|_| "ppm is not running".to_string())?;
        rx.await.map_err(|_| "ppm stopped".to_string())?
    }
}

/// Starts the connection. Dropping the `Client` stops it.
pub fn connect(path: PathBuf) -> (Client, mpsc::UnboundedReceiver<Update>) {
    let (requests, mut calls) = mpsc::unbounded_channel();
    let (updates, rx) = mpsc::unbounded_channel();
    tauri::async_runtime::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            let _ = updates.send(Update::Connecting);
            let (error, connected) = run(&path, &mut calls, &updates).await;
            if calls.is_closed() {
                return;
            }
            if connected {
                backoff = Duration::from_secs(1);
            }
            let _ = updates.send(Update::Disconnected {
                error,
                retry_in: backoff,
            });
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });
    (Client { requests }, rx)
}

/// One run of the process. Returns why it ended and whether it said hello.
async fn run(
    path: &PathBuf,
    calls: &mut mpsc::UnboundedReceiver<(Call, Pending)>,
    updates: &mpsc::UnboundedSender<Update>,
) -> (String, bool) {
    let mut command = Command::new(path);
    command
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return (format!("{}: {error}", path.display()), false),
    };
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut next_id = 1;
    let mut connected = false;

    let error = loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => match serde_json::from_str::<Event>(&line) {
                    Ok(Event::Result(result)) => {
                        if let Some(reply) = pending.remove(&result.id) {
                            let _ = reply.send(result.error.map_or(Ok(()), Err));
                        }
                    }
                    Ok(event) => {
                        connected |= matches!(event, Event::Hello(_));
                        let _ = updates.send(Update::Event(event));
                    }
                    Err(error) => eprintln!("ppm: skipped a line it could not read: {error}"),
                },
                Ok(None) => break "ppm exited".to_string(),
                Err(error) => break error.to_string(),
            },
            call = calls.recv() => {
                let Some((call, reply)) = call else { break "closed".to_string() };
                let id = next_id;
                next_id += 1;
                let mut line = serde_json::to_string(&Request { id, call }).expect("request serializes");
                line.push('\n');
                if let Err(error) = stdin.write_all(line.as_bytes()).await {
                    let _ = reply.send(Err(error.to_string()));
                    break error.to_string();
                }
                pending.insert(id, reply);
            }
        }
    };
    let _ = child.kill().await;
    for (_, reply) in pending {
        let _ = reply.send(Err(error.clone()));
    }
    (error, connected)
}
