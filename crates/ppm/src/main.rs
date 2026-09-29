//! `ppm`: the CLI, terminal UI, and protocol server. See README.md for the
//! command surface this binary grows into.

fn main() {
    let hello = ppm_core::protocol::PROTOCOL_VERSION;
    println!("ppm {} (protocol {hello})", env!("CARGO_PKG_VERSION"));
}
