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
    for (wildcard, other_family) in [("0.0.0.0", "[::1]"), ("::", "127.0.0.1")] {
        let elsewhere = TcpListener::bind(format!("{other_family}:0")).unwrap();
        let outgoing = TcpStream::connect(elsewhere.local_addr().unwrap()).unwrap();
        let port = outgoing.local_addr().unwrap().port();
        let mut server = Server::start(wildcard, port);
        let client = if wildcard == "::" { "::1" } else { "127.0.0.1" };
        let clients: Vec<TcpStream> = (0..2)
            .map(|_| TcpStream::connect((client, port)).unwrap())
            .collect();
        server.wait_for("accepted");

        let counts = native().connections().expect("connections are known");
        drop((clients, outgoing, elsewhere));

        assert_eq!(counts.get(&port), Some(&2), "listener on {wildcard}");
    }
}

/// A Python server in its own process that accepts two connections and holds
/// them until it is dropped. An IPv6 wildcard is IPv6 only, as on Windows, so
/// it can share a port number with an IPv4 socket on every OS.
struct Server {
    child: Child,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(address: &str, port: u16) -> Self {
        const SCRIPT: &str = r#"
import socket, sys
address, port = sys.argv[1], int(sys.argv[2])
family = socket.AF_INET6 if ":" in address else socket.AF_INET
server = socket.socket(family)
if family == socket.AF_INET6:
    server.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
server.bind((address, port))
server.listen()
print("listening", flush=True)
accepted = [server.accept()[0] for _ in range(2)]
print("accepted", flush=True)
sys.stdin.read()
"#;
        let python = if cfg!(windows) { "python" } else { "python3" };
        let mut child = Command::new(python)
            .args(["-c", SCRIPT, address, &port.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("python on PATH");
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut server = Self { child, stdout };
        server.wait_for("listening");
        server
    }

    fn wait_for(&mut self, line: &str) {
        let mut read = String::new();
        self.stdout.read_line(&mut read).unwrap();
        assert_eq!(read.trim(), line, "server exited early");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
