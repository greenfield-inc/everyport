//! When to try a machine again: after a wait that doubles up to a minute,
//! or as soon as this computer's network changes.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

const FIRST: Duration = Duration::from_secs(1);
const LAST: Duration = Duration::from_secs(60);
/// How often a wait looks at the network.
const LOOK: Duration = Duration::from_secs(2);

/// Waits of 1, 2, 4 and so on seconds, up to 60.
#[derive(Debug, Clone)]
pub struct Backoff {
    next: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self { next: FIRST }
    }
}

impl Backoff {
    /// The wait before the next try.
    pub fn next_wait(&mut self) -> Duration {
        let wait = self.next;
        self.next = (self.next * 2).min(LAST);
        wait
    }

    /// Starts over from a second, after a connection worked.
    pub fn reset(&mut self) {
        self.next = FIRST;
    }
}

/// Sleeps for `wait`, or less when this computer's addresses change, such as
/// after joining Wi-Fi or a VPN.
pub async fn wait(wait: Duration) {
    let before = addresses();
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + LOOK)).await;
        if addresses() != before {
            return;
        }
    }
}

fn addresses() -> BTreeSet<IpAddr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|interface| interface.ip())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_up_to_a_minute_and_starts_over() {
        let mut backoff = Backoff::default();
        let waits: Vec<u64> = (0..8).map(|_| backoff.next_wait().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60]);
        backoff.reset();
        assert_eq!(backoff.next_wait(), Duration::from_secs(1));
    }
}
