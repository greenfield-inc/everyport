//! The OS-independent scanner: turns `Platform` facts into protocol
//! `Snapshot`s, keeps per-server history, decides status and clean up, raises
//! alerts, and runs stop and restart. Owned by the engine lane.

mod alerts;
mod command;
mod control;
mod tree;

use crate::links;
use crate::platform::{Platform, ProcDetails, ProcInfo};
use crate::protocol::{
    AgentSession, Alert, Call, CleanUpReason, Config, HostInfo, ProcRef, Project, Sample, Server,
    ServerProcess, ServerStatus, Snapshot, SystemStats, Workspace,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use tree::Table;

/// How far back `Server.history` reaches.
const HISTORY_MS: u64 = 10 * 60 * 1000;
/// Leak growth counts only once history spans this long.
const LEAK_MIN_SPAN_MS: u64 = 2 * 60 * 1000;
/// A server with no connections and less CPU than this is idle.
const ACTIVE_CPU_PERCENT: f32 = 2.0;
/// Status turns idle after this long without activity.
const IDLE_STATUS_MS: u64 = 60 * 60 * 1000;

pub struct Engine {
    platform: Box<dyn Platform>,
    config: Config,
    /// Per process, since a pid can be reused.
    details: HashMap<ProcRef, Option<Arc<ProcDetails>>>,
    /// Last CPU time and when it was read, per process.
    cpu: HashMap<ProcRef, (u64, u64)>,
    /// Workspace and agent session, per root process.
    links: HashMap<ProcRef, (Option<Workspace>, Option<AgentSession>)>,
    tracked: HashMap<(u16, ProcRef), Tracked>,
    alerts: alerts::Alerts,
}

struct Tracked {
    history: Vec<Sample>,
    last_active: u64,
}

impl Engine {
    pub fn new(platform: Box<dyn Platform>, config: Config) -> Self {
        Self {
            platform,
            config,
            details: HashMap::new(),
            cpu: HashMap::new(),
            links: HashMap::new(),
            tracked: HashMap::new(),
            alerts: alerts::Alerts::default(),
        }
    }

    pub fn host(&self) -> HostInfo {
        crate::host::info()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Scan once and return the full state plus any new alerts.
    pub fn scan(&mut self) -> (Snapshot, Vec<Alert>) {
        let now = self.platform.now_ms();
        let table = Table::new(self.platform.processes().unwrap_or_default());
        let connections = self.platform.connections();
        let range = self.config.min_port..=self.config.max_port;
        let own_pid = std::process::id();

        // One server per port: the first process listening on it, with every
        // address that process bound.
        let mut ports: BTreeMap<u16, (u32, Vec<String>)> = BTreeMap::new();
        let listeners = self.platform.listeners().unwrap_or_default();
        for l in listeners.into_iter().filter(|l| range.contains(&l.port)) {
            let (pid, addresses) = ports.entry(l.port).or_insert((l.pid, Vec::new()));
            if *pid == l.pid && !addresses.contains(&l.address) {
                addresses.push(l.address);
            }
        }

        let mut usage = HashMap::new();
        let mut servers = Vec::new();
        for (port, (pid, addresses)) in ports {
            let Some(listener) = table.get(pid).filter(|_| pid != own_pid) else {
                continue;
            };
            let connections = connections.get(&port).copied().unwrap_or(0);
            let seen = Seen {
                port,
                listener,
                addresses,
                connections,
            };
            if let Some(server) = self.server(seen, &table, &mut usage, now) {
                servers.push(server);
            }
        }

        let live_roots: Vec<ProcRef> = servers.iter().map(|s| s.root).collect();
        self.details.retain(|p, _| table.live(*p).is_some());
        self.cpu.retain(|p, _| usage.contains_key(p));
        self.links.retain(|root, _| live_roots.contains(root));
        self.tracked
            .retain(|key, _| servers.iter().any(|s| (s.port, s.root) == *key));
        let alerts = self.alerts.evaluate(&servers, &self.config, now);

        let memory = self.platform.memory();
        let servers_memory: u64 = usage.values().map(|(memory, _)| memory).sum();
        let snapshot = Snapshot {
            taken_at: now,
            system: SystemStats {
                memory_total: memory.total,
                memory_used: memory.used,
                memory_other_apps: memory.used.saturating_sub(servers_memory),
                cpu_percent: self.platform.cpu_percent(),
            },
            servers,
        };
        (snapshot, alerts)
    }

    /// Run a request. `Err` carries a message for `RequestResult.error`.
    pub fn call(&mut self, call: &Call) -> Result<(), String> {
        match call {
            Call::Configure(config) => {
                self.config = config.clone();
                Ok(())
            }
            Call::Refresh => Ok(()),
            Call::Stop { root, force, .. } => self.stop(*root, *force),
            Call::Restart { port, root } => self.restart(*port, *root),
        }
    }

    /// Builds the server behind one listening port. `usage` holds each
    /// process's memory and CPU for this scan, so trees shared by several
    /// ports are measured once.
    fn server(
        &mut self,
        seen: Seen,
        table: &Table,
        usage: &mut HashMap<ProcRef, (u64, f32)>,
        now: u64,
    ) -> Option<Server> {
        let (port, listener) = (seen.port, seen.listener);
        let root = table.root(listener, |p| {
            let details = self.details(p);
            tree::is_agent(p, details.as_deref())
        });
        let listener_details = self.details(listener);
        let root_details = self.details(root);
        // Apps that embed a node or python backend aren't dev servers.
        let in_app = [&listener_details, &root_details].into_iter().any(|d| {
            d.as_ref()
                .and_then(|d| d.args.first())
                .is_some_and(|exe| exe.contains(".app/Contents/"))
        });
        let cwd = self.cwd(listener, root);
        if in_app || cwd.as_deref() == Some("/") {
            return None;
        }

        let members = table.tree(root);
        let mut processes = Vec::with_capacity(members.len());
        for (info, depth) in &members {
            let (memory, cpu_percent) = *usage
                .entry(info.proc)
                .or_insert_with(|| self.measure(info.proc, now));
            let name = match self.details(info) {
                Some(d) => command::display_name(&d.args, &info.name),
                None => info.name.clone(),
            };
            processes.push(ServerProcess {
                proc: info.proc,
                name,
                depth: *depth,
                memory,
                cpu_percent,
            });
        }
        let memory = processes.iter().map(|p| p.memory).sum();
        let cpu_percent = processes.iter().map(|p| p.cpu_percent).sum();

        let tracked = self.tracked.entry((port, root.proc)).or_insert(Tracked {
            history: Vec::new(),
            last_active: now,
        });
        if cpu_percent >= ACTIVE_CPU_PERCENT || seen.connections > 0 {
            tracked.last_active = now;
        }
        tracked.history.push(Sample {
            at: now,
            memory,
            cpu_percent,
        });
        tracked
            .history
            .retain(|s| now.saturating_sub(s.at) <= HISTORY_MS);
        let history = tracked.history.clone();
        let last_active = tracked.last_active;

        let launcher = self.launcher(&members);
        let launch_dir = self.details(launcher).and_then(|d| d.cwd.clone());
        let (workspace, agent) = self.links(table, listener, root, cwd.as_deref());
        let protected = members.iter().any(|(p, _)| {
            let name = tree::stem(&p.name);
            self.config.protected.iter().any(|n| tree::stem(n) == name)
        });

        let mut server = Server {
            port,
            pid: listener.proc.pid,
            root: root.proc,
            process_name: listener.name.clone(),
            addresses: seen.addresses,
            cwd_exists: cwd.as_deref().is_none_or(|c| Path::new(c).exists()),
            command: root_details.map(|d| command::pretty(&d.args, &root.name)),
            launch_dir: launch_dir.or_else(|| cwd.clone()),
            started_at: Some(root.proc.started_at),
            project: match &cwd {
                Some(cwd) => links::project(cwd),
                None => unknown_project(&listener.name),
            },
            workspace,
            agent,
            cwd,
            processes,
            memory,
            cpu_percent,
            connections: seen.connections,
            history,
            last_active,
            protected,
            status: ServerStatus::Running,
            clean_up: None,
        };
        server.status = status(&server, &self.config, now);
        server.clean_up = clean_up(&server, &self.config, now);
        Some(server)
    }

    fn details(&mut self, info: &ProcInfo) -> Option<Arc<ProcDetails>> {
        let platform = &self.platform;
        self.details
            .entry(info.proc)
            .or_insert_with(|| {
                platform
                    .details(info.proc.pid, links::AGENT_ENV_KEYS)
                    .map(Arc::new)
            })
            .clone()
    }

    fn cwd(&mut self, listener: &ProcInfo, root: &ProcInfo) -> Option<String> {
        let of = |d: Option<Arc<ProcDetails>>| d.and_then(|d| d.cwd.clone());
        of(self.details(listener)).or_else(|| of(self.details(root)))
    }

    /// Restart runs from the highest process whose argv wasn't overwritten by
    /// a title, such as the `sh -c "next dev -p 3000"` that npm spawns.
    fn launcher<'a>(&mut self, members: &[(&'a ProcInfo, u32)]) -> &'a ProcInfo {
        let intact = members.iter().find(|(p, _)| {
            self.details(p)
                .is_some_and(|d| command::has_intact_args(&d.args))
        });
        intact.unwrap_or(&members[0]).0
    }

    /// Memory and CPU for one process. CPU is the CPU time used since the
    /// previous scan over the wall time between them.
    fn measure(&mut self, proc: ProcRef, now: u64) -> (u64, f32) {
        let Some(usage) = self.platform.usage(proc.pid) else {
            return (0, 0.0);
        };
        let cpu = match self.cpu.insert(proc, (usage.cpu_time_ns, now)) {
            Some((before, at)) if now > at && usage.cpu_time_ns >= before => {
                (usage.cpu_time_ns - before) as f64 / ((now - at) as f64 * 1e6) * 100.0
            }
            _ => 0.0,
        };
        (usage.memory, cpu as f32)
    }

    /// Workspace and agent session, resolved once per root process.
    fn links(
        &mut self,
        table: &Table,
        listener: &ProcInfo,
        root: &ProcInfo,
        cwd: Option<&str>,
    ) -> (Option<Workspace>, Option<AgentSession>) {
        if let Some(found) = self.links.get(&root.proc) {
            return found.clone();
        }
        // Tools that rename their process (Next.js sets `process.title`)
        // overwrite the memory their environment is read from, so session
        // variables often survive only on a parent. Look two past the root.
        let mut chain = vec![listener];
        let mut extra = 2;
        while let Some(parent) = table.parent(chain[chain.len() - 1]) {
            if chain.len() >= 10 {
                break;
            }
            if chain.iter().any(|p| p.proc == root.proc) {
                if extra == 0 {
                    break;
                }
                extra -= 1;
            }
            chain.push(parent);
        }
        let chain: Vec<ProcDetails> = chain
            .into_iter()
            .map(|p| self.details(p).map(|d| (*d).clone()).unwrap_or_default())
            .collect();
        let found = (cwd.and_then(links::workspace), links::agent(&chain));
        self.links.insert(root.proc, found.clone());
        found
    }
}

/// A port as the listener table reports it.
struct Seen<'a> {
    port: u16,
    listener: &'a ProcInfo,
    addresses: Vec<String>,
    connections: u32,
}

/// For processes whose folder can't be read, such as another user's.
fn unknown_project(name: &str) -> Project {
    Project {
        name: name.to_string(),
        root: None,
        framework: None,
        branch: None,
        worktree: None,
        github: None,
        vercel: None,
    }
}

/// Memory growth across the history window, once it spans two minutes.
fn growth(history: &[Sample]) -> i64 {
    match (history.first(), history.last()) {
        (Some(first), Some(last)) if last.at.saturating_sub(first.at) >= LEAK_MIN_SPAN_MS => {
            last.memory as i64 - first.memory as i64
        }
        _ => 0,
    }
}

fn is_leaking(server: &Server, config: &Config) -> bool {
    growth(&server.history) >= config.leak_growth as i64
}

fn status(server: &Server, config: &Config, now: u64) -> ServerStatus {
    if !server.cwd_exists {
        ServerStatus::Idle
    } else if server.memory >= config.alert_memory || is_leaking(server, config) {
        ServerStatus::Attention
    } else if now.saturating_sub(server.last_active) > IDLE_STATUS_MS {
        ServerStatus::Idle
    } else {
        ServerStatus::Running
    }
}

fn clean_up(server: &Server, config: &Config, now: u64) -> Option<CleanUpReason> {
    if server.protected {
        return None;
    }
    let idle = now.saturating_sub(server.last_active) / 1000;
    let uptime = now.saturating_sub(server.root.started_at) / 1000;
    if !server.cwd_exists {
        Some(CleanUpReason::WorktreeDeleted)
    } else if idle >= config.idle_after_secs {
        Some(CleanUpReason::Idle { seconds: idle })
    } else if uptime >= config.long_running_after_secs {
        Some(CleanUpReason::LongRunning { seconds: uptime })
    } else if is_leaking(server, config) {
        Some(CleanUpReason::Leaking {
            bytes: growth(&server.history) as u64,
        })
    } else {
        None
    }
}
