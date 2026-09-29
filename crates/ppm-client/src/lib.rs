//! Runs `ppm stdio` through a connection (local sidecar, or a command prefix
//! such as `ssh devbox`), or reaches `ppm serve` over HTTP, and turns the
//! stream into `Event`s. The desktop app and `ppm --on` both use it.
//!
//! A remote machine is set up in steps the caller shows to the user:
//! [`install::probe`] it, ask before [`install::install`], then [`connect`]
//! with the probed `ppm_path`.

mod connection;
pub mod discover;
pub mod forward;
mod http;
pub mod install;
pub mod machines;
mod remote;
mod session;
pub mod wsl;

pub use connection::Connection;
pub use ppm_core::protocol;
pub use session::{connect, Client, Update};
