//! Code shared by the macOS and Linux platforms.

use crate::protocol::ProcRef;
use std::io;
use std::sync::Mutex;

/// Whole-machine CPU use from cumulative busy and total tick counters.
#[derive(Default)]
pub(super) struct CpuSampler {
    previous: Mutex<Option<(u64, u64)>>,
}

impl CpuSampler {
    /// Busy share, 0-100, since the previous sample. The first sample is 0.
    pub(super) fn percent(&self, busy: u64, total: u64) -> f32 {
        match self.previous.lock().unwrap().replace((busy, total)) {
            Some((prev_busy, prev_total)) if total > prev_total && busy >= prev_busy => {
                ((busy - prev_busy) as f64 / (total - prev_total) as f64 * 100.0) as f32
            }
            _ => 0.0,
        }
    }
}

/// Sends SIGTERM (SIGKILL with `force`) after checking that `target.pid` still
/// has the start time the caller saw, so a reused pid is never signalled.
pub(super) fn signal(
    target: ProcRef,
    force: bool,
    started_at: impl Fn(u32) -> Option<u64>,
) -> io::Result<()> {
    // 0, 1 and anything that wraps negative would signal a group or init.
    let pid = i32::try_from(target.pid)
        .ok()
        .filter(|&pid| pid > 1)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "refusing to signal this pid")
        })?;
    if started_at(target.pid) != Some(target.started_at) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "process has exited or its pid was reused",
        ));
    }
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Keeps the `KEY=value` entries whose key is in `keys`.
pub(super) fn pick_env(
    entries: impl Iterator<Item = String>,
    keys: &[&str],
) -> Vec<(String, String)> {
    entries
        .filter_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            keys.contains(&key)
                .then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}
