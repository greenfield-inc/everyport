//! Scanner engine and wire protocol for Everyport.
//!
//! `protocol` is the contract every client builds on. `platform` is the only
//! place OS-specific code lives. `engine` and `links` are shared by all OSes.

pub mod config;
pub mod engine;
pub mod host;
pub mod links;
pub mod platform;
pub mod protocol;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
