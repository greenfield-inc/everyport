//! Alert episodes. Each alert fires once and re-arms only after its condition
//! clears, so a server hovering around a threshold doesn't nag.

use super::growth;
use crate::protocol::{Alert, AlertKind, Config, ProcRef, Server};
use std::collections::HashMap;

/// How long a server stays over the memory threshold before it alerts.
const SUSTAIN_MS: u64 = 30_000;

#[derive(Default)]
pub struct Alerts {
    episodes: HashMap<(u16, ProcRef), Episode>,
}

#[derive(Default)]
struct Episode {
    over_since: Option<u64>,
    memory_fired: bool,
    leak_fired: bool,
}

impl Alerts {
    pub fn evaluate(&mut self, servers: &[Server], config: &Config, now: u64) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let mut live = HashMap::new();
        for server in servers.iter().filter(|s| !s.protected) {
            let key = (server.port, server.root);
            let mut episode = self.episodes.remove(&key).unwrap_or_default();
            let mut fire = |kind| {
                alerts.push(Alert {
                    port: server.port,
                    kind,
                    memory: server.memory,
                })
            };

            // Re-arm below 90% of the threshold to avoid flapping.
            if server.memory >= config.alert_memory {
                let since = *episode.over_since.get_or_insert(now);
                if now.saturating_sub(since) >= SUSTAIN_MS && !episode.memory_fired {
                    episode.memory_fired = true;
                    fire(AlertKind::OverThreshold);
                }
            } else if (server.memory as f64) < config.alert_memory as f64 * 0.9 {
                episode.over_since = None;
                episode.memory_fired = false;
            }

            // Re-arm once growth drops under half the trigger.
            let growth = growth(&server.history);
            if growth >= config.leak_growth as i64 && !episode.leak_fired {
                episode.leak_fired = true;
                fire(AlertKind::Leaking);
            } else if growth < (config.leak_growth / 2) as i64 {
                episode.leak_fired = false;
            }

            live.insert(key, episode);
        }
        self.episodes = live;
        alerts
    }
}
