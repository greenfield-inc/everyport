//! The process table for one scan: parents, children, and the tree behind a
//! listening port.

use crate::platform::{ProcDetails, ProcInfo};
use crate::protocol::ProcRef;
use std::collections::HashMap;

/// Processes that sit between a shell and the actual server, such as `npm run dev`.
const RUNNERS: &[&str] = &[
    "node", "npm", "npx", "pnpm", "yarn", "bun", "bunx", "deno", "turbo", "nx", "python",
    "python3", "uv", "poetry", "pipenv", "ruby", "bundle", "rails", "go", "air", "cargo", "java",
    "gradle", "mvn", "dotnet", "php", "mix", "beam.smp", "elixir",
];
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "cmd",
    "powershell",
    "pwsh",
];
const AGENTS: &[&str] = &["claude", "codex", "conductor"];
/// Path fragments that identify an agent run through a runtime like node.
const AGENT_PATH_MARKERS: &[&str] = &["@anthropic-ai/claude-code", "com.conductor.app", "/codex/"];

pub struct Table {
    procs: HashMap<u32, ProcInfo>,
    children: HashMap<u32, Vec<u32>>,
}

impl Table {
    pub fn new(list: Vec<ProcInfo>) -> Self {
        let mut table = Self {
            procs: list.into_iter().map(|p| (p.proc.pid, p)).collect(),
            children: HashMap::new(),
        };
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for info in table.procs.values() {
            if let Some(parent) = table.parent(info) {
                children
                    .entry(parent.proc.pid)
                    .or_default()
                    .push(info.proc.pid);
            }
        }
        children.values_mut().for_each(|pids| pids.sort_unstable());
        table.children = children;
        table
    }

    pub fn get(&self, pid: u32) -> Option<&ProcInfo> {
        self.procs.get(&pid)
    }

    /// The process `target` names, if it is still running.
    pub fn live(&self, target: ProcRef) -> Option<&ProcInfo> {
        self.get(target.pid).filter(|p| p.proc == target)
    }

    /// The parent, below init. A recorded parent pid that now belongs to a
    /// younger process was reused, so it is not the parent.
    pub fn parent(&self, info: &ProcInfo) -> Option<&ProcInfo> {
        let pid = info.parent.filter(|&pid| pid > 1)?;
        self.get(pid)
            .filter(|parent| parent.proc.started_at <= info.proc.started_at)
    }

    /// Climbs from the listening process to the command the user ran, such as
    /// from `next-server` up to `npm run dev`. Stops at shells, terminals and
    /// coding agents, so stopping a server never takes its launcher with it.
    pub fn root<'a>(
        &'a self,
        listener: &'a ProcInfo,
        mut is_agent: impl FnMut(&ProcInfo) -> bool,
    ) -> &'a ProcInfo {
        let mut current = listener;
        while let Some(parent) = self.parent(current) {
            if is_runner(parent) && !is_agent(parent) {
                current = parent;
                continue;
            }
            // Package managers run scripts through `sh -c`; step over that
            // shell only when a package manager sits directly above it.
            if is_shell(parent) {
                if let Some(grandparent) = self.parent(parent) {
                    if is_runner(grandparent) && !is_agent(grandparent) {
                        current = grandparent;
                        continue;
                    }
                }
            }
            break;
        }
        current
    }

    /// `root` and all its descendants with their depth, depth-first, root first.
    pub fn tree<'a>(&'a self, root: &'a ProcInfo) -> Vec<(&'a ProcInfo, u32)> {
        let mut result = Vec::new();
        let mut stack = vec![(root, 0)];
        while let Some((info, depth)) = stack.pop() {
            result.push((info, depth));
            let children = self.children.get(&info.proc.pid).into_iter().flatten();
            let children: Vec<_> = children.filter_map(|pid| self.get(*pid)).collect();
            stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
        }
        result
    }
}

/// Lowercased name without a Windows `.exe`, for matching.
pub fn stem(name: &str) -> String {
    let name = name.to_lowercase();
    match name.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => name,
    }
}

fn is_runner(info: &ProcInfo) -> bool {
    RUNNERS.contains(&stem(&info.name).as_str())
}

fn is_shell(info: &ProcInfo) -> bool {
    SHELLS.contains(&stem(&info.name).as_str())
}

pub fn is_agent(info: &ProcInfo, details: Option<&ProcDetails>) -> bool {
    if AGENTS.contains(&stem(&info.name).as_str()) {
        return true;
    }
    let Some(details) = details else {
        return false;
    };
    let joined = details.args.iter().take(4).cloned().collect::<Vec<_>>();
    let joined = joined.join(" ").replace('\\', "/");
    AGENT_PATH_MARKERS.iter().any(|m| joined.contains(m))
}
