//! Forwarding a port over a connection with no forwarding of its own, which
//! relays through `ppm connect` on the machine. An empty prefix runs it here.

use ppm_client::forward::forward;
use ppm_client::Connection;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::thread;

#[tokio::test(flavor = "multi_thread")]
async fn relays_through_ppm_connect() {
    let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = server.local_addr().unwrap().port();
    thread::spawn(move || {
        // Forwarding first checks whether the port answers here, with a
        // connection that sends nothing.
        for mut stream in server.incoming().flatten() {
            let mut request = [0; 4];
            if stream.read_exact(&mut request).is_ok() && &request == b"ping" {
                stream.write_all(b"pong").unwrap();
            }
        }
    });
    let connection = Connection::Command {
        argv_prefix: Vec::new(),
        ppm_path: env!("CARGO_BIN_EXE_ppm").into(),
    };
    let forwarded = forward(&connection, port).await.unwrap();
    assert!(forwarded.is_tunnel());
    // The server holds the port here, so the forward picks another.
    assert_ne!(forwarded.local_port, port);

    let local_port = forwarded.local_port;
    let reply = tokio::task::spawn_blocking(move || {
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).unwrap();
        client.write_all(b"ping").unwrap();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        reply
    })
    .await
    .unwrap();
    assert_eq!(reply, "pong");
}
