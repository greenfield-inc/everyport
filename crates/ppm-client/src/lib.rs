//! Runs `ppm stdio` through a connection (local sidecar, or a command prefix
//! such as `ssh devbox`), or reaches `ppm serve` over HTTP, and turns the
//! stream into `Event`s. The desktop app and `ppm --on` both use it.

pub use ppm_core::protocol;
