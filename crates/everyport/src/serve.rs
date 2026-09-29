//! `everyport serve`: the protocol over HTTP on loopback. `GET /events` streams
//! events as server-sent events and `POST /call` runs one request. Every
//! request needs `Authorization: Bearer <token>`.

use crate::hub::{parse_request, Hub};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:7767";
const TOKEN_FILE: &str = "serve-token";
const MAX_HEAD: u64 = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const PING: Duration = Duration::from_secs(15);

/// Binds only to loopback, so the server is reachable only through a tunnel
/// or proxy the user sets up.
pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
    if !addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{addr} is not a loopback address; everyport serve listens only on loopback, so put a tunnel or proxy in front of it"),
        ));
    }
    TcpListener::bind(addr)
}

/// The token in `dir`, created on first use.
pub fn token(dir: &Path) -> io::Result<String> {
    let path = dir.join(TOKEN_FILE);
    if let Ok(token) = std::fs::read_to_string(&path) {
        if !token.trim().is_empty() {
            return Ok(token.trim().to_string());
        }
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    std::fs::create_dir_all(dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&path)?.write_all(token.as_bytes())?;
    Ok(token)
}

/// `everyport://` and the base64url of `{"url", "token"}`.
pub fn connection_code(url: &str, token: &str) -> String {
    let json = serde_json::json!({ "url": url, "token": token });
    format!("everyport://{}", URL_SAFE_NO_PAD.encode(json.to_string()))
}

/// Who may use the server.
pub struct Access {
    pub token: String,
    /// Web origins, such as `https://dash.example.com`, whose pages may read
    /// responses. Other pages get no CORS headers, so a browser blocks them.
    pub origins: Vec<String>,
}

/// Serves connections until the listener fails.
pub fn run(hub: Hub, listener: TcpListener, access: Access) -> io::Result<()> {
    let access = Arc::new(access);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let (hub, access) = (hub.clone(), access.clone());
        thread::spawn(move || {
            let _ = handle(stream, &hub, &access);
        });
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    origin: Option<String>,
    body: Vec<u8>,
}

fn handle(mut stream: TcpStream, hub: &Hub, access: &Access) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(error) => {
            return respond(
                &mut stream,
                "400 Bad Request",
                &error_body(&error.to_string()),
                "",
            )
        }
    };
    let cors = cors_headers(request.origin.as_deref(), &access.origins);
    // CORS preflight carries no credentials. The token guards everything else.
    if request.method == "OPTIONS" {
        return write_head(&mut stream, "204 No Content", "text/plain", Some(0), &cors);
    }
    if !authorized(request.authorization.as_deref(), &access.token) {
        return respond(
            &mut stream,
            "401 Unauthorized",
            &error_body("missing or wrong bearer token"),
            &cors,
        );
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/events") => stream_events(stream, hub, &cors),
        ("POST", "/call") => {
            let (status, event) = match parse_request(&String::from_utf8_lossy(&request.body)) {
                Ok(call) => {
                    let (reply, rx) = mpsc::channel();
                    hub.call(call, reply);
                    let event = rx.recv().map_err(|_| io::Error::other("scanner stopped"))?;
                    ("200 OK", event)
                }
                Err(event) => ("400 Bad Request", event),
            };
            respond(&mut stream, status, &serde_json::to_string(&event)?, &cors)
        }
        (_, "/events" | "/call") => respond(
            &mut stream,
            "405 Method Not Allowed",
            &error_body("method not allowed"),
            &cors,
        ),
        _ => respond(
            &mut stream,
            "404 Not Found",
            &error_body("not found"),
            &cors,
        ),
    }
}

fn read_request(stream: &TcpStream) -> io::Result<HttpRequest> {
    let mut reader = BufReader::new(stream.take(MAX_HEAD));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bad request line",
        ));
    };
    let method = method.to_string();
    let path = target.split('?').next().unwrap_or_default().to_string();

    let mut authorization = None;
    let mut origin = None;
    let mut length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "headers too long or cut off",
            ));
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value
                .parse()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad content-length"))?;
        }
    }
    if length > MAX_BODY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    // The head limit no longer applies to the body.
    let buffered = reader.buffer().to_vec();
    let mut body = buffered[..buffered.len().min(length)].to_vec();
    if body.len() < length {
        let mut rest = vec![0; length - body.len()];
        let mut raw = stream;
        raw.read_exact(&mut rest)?;
        body.extend(rest);
    }
    Ok(HttpRequest {
        method,
        path,
        authorization,
        origin,
        body,
    })
}

fn authorized(header: Option<&str>, token: &str) -> bool {
    let Some((scheme, value)) = header.and_then(|h| h.split_once(' ')) else {
        return false;
    };
    let (a, b) = (value.trim().as_bytes(), token.as_bytes());
    // Compare in constant time.
    scheme.eq_ignore_ascii_case("bearer")
        && a.len() == b.len()
        && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// CORS headers for `origin` when it's allowed, and none otherwise.
fn cors_headers(origin: Option<&str>, allowed: &[String]) -> String {
    match origin {
        Some(origin) if allowed.iter().any(|a| a.trim_end_matches('/') == origin) => format!(
            "Access-Control-Allow-Origin: {origin}\r\n\
             Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
             Access-Control-Allow-Methods: GET, POST\r\n\
             Vary: Origin\r\n"
        ),
        _ => String::new(),
    }
}

fn stream_events(mut stream: TcpStream, hub: &Hub, cors: &str) -> io::Result<()> {
    write_head(&mut stream, "200 OK", "text/event-stream", None, cors)?;
    let (events, rx) = mpsc::channel();
    hub.subscribe(events);
    loop {
        match rx.recv_timeout(PING) {
            Ok(event) => write!(stream, "data: {}\n\n", serde_json::to_string(&event)?)?,
            // Keeps proxies from closing an idle stream, and finds dead clients.
            Err(mpsc::RecvTimeoutError::Timeout) => stream.write_all(b": ping\n\n")?,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        stream.flush()?;
    }
}

fn error_body(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

fn respond(stream: &mut TcpStream, status: &str, json: &str, cors: &str) -> io::Result<()> {
    write_head(stream, status, "application/json", Some(json.len()), cors)?;
    stream.write_all(json.as_bytes())?;
    stream.flush()
}

fn write_head(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    length: Option<usize>,
    cors: &str,
) -> io::Result<()> {
    let length = length.map_or(String::new(), |n| format!("Content-Length: {n}\r\n"));
    write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         {length}\
         Cache-Control: no-cache\r\n\
         X-Accel-Buffering: no\r\n\
         {cors}\
         Connection: close\r\n\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::fixture;
    use everyport::protocol::{Event, RequestResult};

    const TOKEN: &str = "s3cret";

    const DASHBOARD: &str = "https://dash.example.com";

    fn server() -> SocketAddr {
        let listener = bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let access = Access {
            token: TOKEN.into(),
            origins: vec![format!("{DASHBOARD}/")],
        };
        thread::spawn(move || run(fixture::hub(), listener, access));
        addr
    }

    /// Sends a raw request and returns the status line and body.
    fn request(addr: SocketAddr, head: &str, body: &str) -> (String, String) {
        let (head, body) = exchange(addr, head, body);
        (head.lines().next().unwrap().to_string(), body)
    }

    /// Sends a raw request and returns the response head and body.
    fn exchange(addr: SocketAddr, head: &str, body: &str) -> (String, String) {
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "{head}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        (head.to_string(), body.to_string())
    }

    fn post_call(addr: SocketAddr, body: &str) -> (String, Event) {
        let head = format!("POST /call HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}");
        let (status, body) = request(addr, &head, body);
        (status, serde_json::from_str(&body).unwrap())
    }

    #[test]
    fn call_returns_its_result() {
        let addr = server();
        let (status, event) = post_call(addr, r#"{"id":4,"method":"refresh"}"#);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(event, Event::Result(RequestResult { id: 4, error: None }));

        let (status, event) = post_call(addr, r#"{"id":5,"method":"launch"}"#);
        assert_eq!(status, "HTTP/1.1 400 Bad Request");
        assert!(matches!(
            event,
            Event::Result(RequestResult {
                id: 5,
                error: Some(_)
            })
        ));
    }

    #[test]
    fn every_request_needs_the_token() {
        let addr = server();
        for auth in [
            "",
            "\r\nAuthorization: Bearer wrong",
            "\r\nAuthorization: Basic s3cret",
        ] {
            let (status, _) = request(addr, &format!("GET /events HTTP/1.1\r\nHost: x{auth}"), "");
            assert_eq!(status, "HTTP/1.1 401 Unauthorized");
        }
        let (status, _) = request(
            addr,
            &format!("GET /nope HTTP/1.1\r\nAuthorization: Bearer {TOKEN}"),
            "",
        );
        assert_eq!(status, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn events_stream_hello_then_the_snapshot() {
        let addr = server();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "GET /events HTTP/1.1\r\nAuthorization: bearer {TOKEN}\r\n\r\n"
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line.trim_end(), "HTTP/1.1 200 OK");
        let mut events = Vec::new();
        while events.len() < 2 {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if let Some(json) = line.strip_prefix("data: ") {
                events.push(serde_json::from_str::<Event>(json).unwrap());
            }
        }
        assert!(matches!(events[0], Event::Hello(_)));
        assert_eq!(events[1], Event::Snapshot(fixture::snapshot()));
    }

    #[test]
    fn only_allowed_origins_get_cors_headers() {
        let addr = server();
        let call = format!("POST /call HTTP/1.1\r\nAuthorization: Bearer {TOKEN}");
        let body = r#"{"id":1,"method":"refresh"}"#;
        let (head, _) = exchange(
            addr,
            &format!("{call}\r\nOrigin: https://evil.example"),
            body,
        );
        assert!(!head.contains("Access-Control"));
        let (head, _) = exchange(addr, &format!("{call}\r\nOrigin: {DASHBOARD}"), body);
        assert!(head.contains(&format!("Access-Control-Allow-Origin: {DASHBOARD}\r\n")));
        let preflight = format!("OPTIONS /call HTTP/1.1\r\nOrigin: {DASHBOARD}");
        let (head, _) = exchange(addr, &preflight, "");
        assert!(head.starts_with("HTTP/1.1 204"));
        assert!(head.contains("Access-Control-Allow-Headers: Authorization"));
    }

    #[test]
    fn refuses_to_listen_beyond_loopback() {
        assert!(bind("0.0.0.0:0".parse().unwrap()).is_err());
        // Some CI runners have no IPv6 loopback.
        if TcpListener::bind("[::1]:0").is_ok() {
            assert!(bind("[::1]:0".parse().unwrap()).is_ok());
        }
    }

    #[test]
    fn code_carries_url_and_token() {
        let code = connection_code("https://devbox.tail1234.ts.net", "abc");
        let json = URL_SAFE_NO_PAD
            .decode(code.strip_prefix("everyport://").unwrap())
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "url": "https://devbox.tail1234.ts.net", "token": "abc" })
        );
    }

    #[test]
    fn token_is_created_once_and_private() {
        let dir = std::env::temp_dir().join(format!("everyport-token-test-{}", std::process::id()));
        let first = token(&dir).unwrap();
        assert_eq!(token(&dir).unwrap(), first);
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), 32);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(TOKEN_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
