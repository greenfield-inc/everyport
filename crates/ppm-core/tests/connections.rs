//! Inbound connection counts on every OS. An outgoing connection can use a
//! listener's port number as its local port, as the CI runner's own
//! connections did. It is not an inbound one.

use ppm_core::platform::native;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, ChildStdout, Command, Stdio};

#[test]
fn outgoing_socket_on_another_address_is_not_counted() {
    let elsewhere = TcpListener::bind("[::1]:0").unwrap();
    let outgoing = TcpStream::connect(elsewhere.local_addr().unwrap()).unwrap();
    let port = outgoing.local_addr().unwrap().port();
    let server = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let clients: Vec<TcpStream> = (0..2)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    let accepted: Vec<TcpStream> = (0..2).map(|_| server.accept().unwrap().0).collect();

    let counts = native().connections().expect("connections are known");
    drop((clients, accepted, outgoing, elsewhere));

    assert_eq!(counts.get(&port), Some(&2));
}

/// A server on `0.0.0.0` or `::` accepts on every address of its family, so
/// the address can't rule out another process's outgoing socket on its port.
#[test]
fn outgoing_socket_of_another_process_is_not_counted_on_a_wildcard_listener() {
    for (wildcard, other_family) in [("0.0.0.0", "::1"), ("::", "127.0.0.1")] {
        let mut outgoing = Python::start(&["outgoing", other_family]);
        let port = outgoing.line();
        let mut server = Python::start(&["serve", wildcard, &port]);
        assert_eq!(server.line(), "listening");
        let port: u16 = port.parse().unwrap();
        let client = if wildcard == "::" { "::1" } else { "127.0.0.1" };
        let clients: Vec<TcpStream> = (0..2)
            .map(|_| TcpStream::connect((client, port)).unwrap())
            .collect();
        assert_eq!(server.line(), "accepted");

        let counts = native().connections().expect("connections are known");
        drop(clients);

        assert_eq!(counts.get(&port), Some(&2), "listener on {wildcard}");
    }
}

/// A prefork server (gunicorn, or a Node cluster on Unix) listens in one
/// process and accepts in the workers it forks.
#[cfg(unix)]
#[test]
fn connections_accepted_by_a_forked_worker_are_counted() {
    let mut server = Python::start(&["fork", "0.0.0.0", "0"]);
    let port: u16 = server.line().parse().unwrap();
    let clients: Vec<TcpStream> = (0..2)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();
    assert_eq!(server.line(), "accepted");

    let counts = native().connections().expect("connections are known");
    drop(clients);

    assert_eq!(counts.get(&port), Some(&2));
}

/// A Python process that holds its sockets until it is dropped. `outgoing`
/// connects from `address` and prints its local port. `serve` listens on
/// `address` and the given port, and accepts two connections. `fork` does
/// the same, but prints its port and accepts in a forked child, which then
/// closes its copy of the listener. IPv6 sockets
/// are IPv6 only, as on Windows, so an IPv4 socket can share their port
/// number on every OS.
struct Python {
    child: Child,
    stdout: BufReader<ChildStdout>,
}

impl Python {
    fn start(args: &[&str]) -> Self {
        const SCRIPT: &str = r#"
import os, socket, sys
def tcp(address):
    family = socket.AF_INET6 if ":" in address else socket.AF_INET
    s = socket.socket(family)
    if family == socket.AF_INET6:
        s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
    return s
mode, address = sys.argv[1], sys.argv[2]
if mode == "outgoing":
    peer = tcp(address)
    peer.bind((address, 0))
    peer.listen()
    out = tcp(address)
    out.connect(peer.getsockname())
    accepted = peer.accept()
    print(out.getsockname()[1], flush=True)
else:
    server = tcp(address)
    server.bind((address, int(sys.argv[3])))
    server.listen()
    if mode == "fork":
        print(server.getsockname()[1], flush=True)
        if os.fork() == 0:
            accepted = [server.accept()[0] for _ in range(2)]
            server.close()
            print("accepted", flush=True)
    else:
        print("listening", flush=True)
        accepted = [server.accept()[0] for _ in range(2)]
        print("accepted", flush=True)
sys.stdin.read()
"#;
        let python = if cfg!(windows) { "python" } else { "python3" };
        let mut child = Command::new(python)
            .arg("-c")
            .arg(SCRIPT)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("python on PATH");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdout }
    }

    fn line(&mut self) -> String {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        line.trim().to_string()
    }
}

impl Drop for Python {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
