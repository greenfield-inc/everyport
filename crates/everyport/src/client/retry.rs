//! When to try a machine again: after a wait that doubles up to a minute,
//! or as soon as this computer joins a network.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
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

/// Sleeps for `wait`, or less when this computer gets a new address, such
/// as after joining Wi-Fi or a VPN. The address must still be there a look
/// later, so a flapping interface doesn't cut every wait short.
pub async fn wait(wait: Duration) {
    let before = addresses();
    let mut new_last_look = BTreeSet::new();
    let deadline = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + LOOK)).await;
        let new: BTreeSet<Ipv4Addr> = addresses().difference(&before).copied().collect();
        if new.intersection(&new_last_look).next().is_some() {
            return;
        }
        new_last_look = new;
    }
}

/// This computer's routable IPv4 addresses. IPv6 is left out, since
/// temporary addresses rotate on their own, and so are link-local ones.
fn addresses() -> BTreeSet<Ipv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(ip),
            _ => None,
        })
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
