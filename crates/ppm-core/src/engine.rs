//! The OS-independent scanner: turns `Platform` facts into protocol
//! `Snapshot`s, keeps per-server history, decides status and clean up, raises
//! alerts, and runs stop and restart. Owned by the engine lane.

use crate::platform::Platform;
use crate::protocol::{Alert, Call, Config, HostInfo, Snapshot, SystemStats};

pub struct Engine {
    platform: Box<dyn Platform>,
    config: Config,
}

impl Engine {
    pub fn new(platform: Box<dyn Platform>, config: Config) -> Self {
        Self { platform, config }
    }

    pub fn host(&self) -> HostInfo {
        crate::host::info()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Scan once and return the full state plus any new alerts.
    pub fn scan(&mut self) -> (Snapshot, Vec<Alert>) {
        let memory = self.platform.memory();
        let snapshot = Snapshot {
            taken_at: crate::now_ms(),
            system: SystemStats {
                memory_total: memory.total,
                memory_used: memory.used,
                memory_other_apps: memory.used,
                cpu_percent: self.platform.cpu_percent(),
            },
            servers: Vec::new(),
        };
        (snapshot, Vec::new())
    }

    /// Run a request. `Err` carries a message for `RequestResult.error`.
    pub fn call(&mut self, call: &Call) -> Result<(), String> {
        match call {
            Call::Configure(config) => {
                self.config = config.clone();
                Ok(())
            }
            Call::Refresh => Ok(()),
            Call::Stop { .. } | Call::Restart { .. } => Err("not implemented yet".into()),
        }
    }
}
