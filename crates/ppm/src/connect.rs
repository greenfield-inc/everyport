//! `ppm connect <port>`: pipes stdin and stdout to a server on this machine.
//! ppm-client runs it through a connection such as `docker exec -i` to
//! forward a port that the connection can't forward itself.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::process::ExitCode;
use std::thread;

/// With `check`, exits as soon as the connection is made.
pub fn run(port: u16, check: bool) -> io::Result<ExitCode> {
    let stream = TcpStream::connect(("localhost", port))
        .map_err(|e| io::Error::new(e.kind(), format!("nothing answers on :{port}: {e}")))?;
    if check {
        return Ok(ExitCode::SUCCESS);
    }
    let mut upload = stream.try_clone()?;
    thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut upload);
        let _ = upload.shutdown(Shutdown::Write);
    });
    // Stdout buffers by line, so each chunk is flushed as it arrives.
    let mut download = stream;
    let mut out = io::stdout().lock();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = download.read(&mut buffer)?;
        if n == 0 {
            return Ok(ExitCode::SUCCESS);
        }
        out.write_all(&buffer[..n])?;
        out.flush()?;
    }
}
