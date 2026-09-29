//! A session with `everyport serve`: events arrive as server-sent events on
//! `GET /events`, and each request is a `POST /call` whose body is its result.

use crate::client::session::{Ended, Reply, Session};
use crate::protocol::{Call, Event, Request};
use reqwest::{header, StatusCode};
use std::time::Duration;

const TOKEN_REJECTED: &str =
    "The machine rejected the connection code. Run `everyport serve` there and add the new code.";

/// A client builder that uses ring for TLS.
pub(crate) fn builder() -> reqwest::ClientBuilder {
    // Fails harmlessly when the process already chose a provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

pub(crate) async fn run(url: &str, token: &str, session: &mut Session<'_>) -> Ended {
    let http = match builder()
        .connect_timeout(Duration::from_secs(10))
        // `everyport serve` sends a keep-alive comment every 15 s.
        .read_timeout(Duration::from_secs(45))
        .build()
    {
        Ok(http) => http,
        Err(e) => return Ended::Error(e.to_string()),
    };
    let response = http
        .get(format!("{url}/events"))
        .bearer_auth(token)
        .header(header::ACCEPT, "text/event-stream")
        .send()
        .await;
    let mut response = match response {
        Ok(r) if r.status() == StatusCode::UNAUTHORIZED => {
            return Ended::Error(TOKEN_REJECTED.into())
        }
        Ok(r) if !r.status().is_success() => {
            return Ended::Error(format!("{url} answered {}.", r.status()))
        }
        Ok(r) => r,
        Err(e) => return Ended::Error(format!("Couldn't reach {url}: {e}")),
    };
    let mut next_id = 1;
    let mut buffer = Vec::new();
    let mut data = String::new();
    loop {
        tokio::select! {
            chunk = response.chunk() => match chunk {
                Ok(Some(bytes)) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = buffer.drain(..=end).collect();
                        let line = String::from_utf8_lossy(&line);
                        let line = line.trim_end_matches(['\n', '\r']);
                        if let Some(value) = line.strip_prefix("data:") {
                            data.push_str(value.strip_prefix(' ').unwrap_or(value));
                        } else if line.is_empty() && !data.is_empty() {
                            let event = serde_json::from_str::<Event>(&std::mem::take(&mut data));
                            if let Ok(event) = event {
                                if let Some(ended) = session.event(event) {
                                    return ended;
                                }
                            }
                        }
                    }
                }
                Ok(None) => return Ended::Error(format!("{url} closed the connection.")),
                Err(e) => return Ended::Error(format!("Lost {url}: {e}")),
            },
            call = session.calls.recv() => {
                let Some((call, reply)) = call else { return Ended::Stopped };
                tokio::spawn(post(http.clone(), format!("{url}/call"), token.to_string(), next_id, call, reply));
                next_id += 1;
            }
        }
    }
}

async fn post(
    http: reqwest::Client,
    url: String,
    token: String,
    id: u64,
    call: Call,
    reply: Reply,
) {
    let result = async {
        let body = serde_json::to_vec(&Request { id, call }).map_err(|e| e.to_string())?;
        let response = http
            .post(&url)
            .bearer_auth(token)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Couldn't reach {url}: {e}"))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(TOKEN_REJECTED.to_string());
        }
        let body = response.bytes().await.map_err(|e| e.to_string())?;
        match serde_json::from_slice::<Event>(&body) {
            Ok(Event::Result(result)) => result.error.map_or(Ok(()), Err),
            _ => Err(format!("{url} answered {status}.")),
        }
    }
    .await;
    let _ = reply.send(result);
}
