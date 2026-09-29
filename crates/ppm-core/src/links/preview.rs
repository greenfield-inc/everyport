//! Vercel preview URLs through the GitHub CLI. Vercel's GitHub integration
//! records every preview as a GitHub deployment, so no Vercel token is
//! needed. Lookups run on a background thread; callers get the last result.

use super::watch::UNUSED_FOR;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const MAX_AGE: Duration = Duration::from_secs(120);

static ENABLED: AtomicBool = AtomicBool::new(false);
static PREVIEWS: LazyLock<Mutex<HashMap<(String, String), Entry>>> = LazyLock::new(Mutex::default);

struct Entry {
    url: Option<String>,
    fetched_at: Option<Instant>,
    in_flight: bool,
    used: Instant,
}

/// Previews go online, so they stay off until the user turns them on.
pub fn set_vercel_previews(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// The cached preview for `repo`'s `branch`, refreshed in the background once
/// it is older than two minutes.
pub(super) fn preview_url(root: &Path, repo: &str, branch: &str) -> Option<String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let key = (repo.to_string(), branch.to_string());
    let now = Instant::now();
    let mut previews = PREVIEWS.lock().unwrap();
    previews.retain(|_, e| e.in_flight || now.duration_since(e.used) < UNUSED_FOR);
    let entry = previews.entry(key.clone()).or_insert(Entry {
        url: None,
        fetched_at: None,
        in_flight: false,
        used: now,
    });
    entry.used = now;
    if !entry.in_flight && entry.fetched_at.is_none_or(|at| at.elapsed() > MAX_AGE) {
        entry.in_flight = true;
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            let url = lookup(&root, &key.0, &key.1);
            let now = Instant::now();
            let fetched = Entry {
                url,
                fetched_at: Some(now),
                in_flight: false,
                used: now,
            };
            PREVIEWS.lock().unwrap().insert(key, fetched);
        });
    }
    entry.url.clone()
}

/// The newest successful preview deployment among the branch's last 30 commits.
fn lookup(root: &Path, repo: &str, branch: &str) -> Option<String> {
    let commits = gh(
        root,
        &[
            &format!("repos/{repo}/commits?sha={}&per_page=30", encode(branch)),
            "--jq",
            ".[].sha",
        ],
    )?;
    let commits: Vec<&str> = commits.lines().collect();
    let deployments = json(gh(
        root,
        &[&format!("repos/{repo}/deployments?per_page=50")],
    )?)?;
    let (_, id) = deployments
        .as_array()?
        .iter()
        .filter(|d| {
            d["environment"]
                .as_str()
                .is_some_and(|e| e.to_lowercase().contains("preview"))
        })
        .filter_map(|d| {
            Some((
                commits.iter().position(|c| Some(*c) == d["sha"].as_str())?,
                d["id"].as_u64()?,
            ))
        })
        // The newest commit first, then the newest deployment of it.
        .min_by_key(|&(rank, id)| (rank, std::cmp::Reverse(id)))?;
    let statuses = json(gh(
        root,
        &[&format!(
            "repos/{repo}/deployments/{id}/statuses?per_page=1"
        )],
    )?)?;
    let status = statuses.get(0).filter(|s| s["state"] == "success")?;
    ["environment_url", "target_url"]
        .iter()
        .filter_map(|k| status[k].as_str())
        .find(|url| !url.is_empty())
        .map(str::to_string)
}

fn json(text: String) -> Option<serde_json::Value> {
    serde_json::from_str(&text).ok()
}

/// Runs `gh api <args>`. Apps launched from the Dock or Start menu may not
/// have Homebrew on their PATH, so the usual install folders are tried too.
fn gh(dir: &Path, args: &[&str]) -> Option<String> {
    let output = ["gh", "/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
        .iter()
        .find_map(|gh| {
            Command::new(gh)
                .arg("api")
                .args(args)
                .current_dir(dir)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()
        })?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Percent-encodes a branch name for a query string. `/` stays as is.
fn encode(branch: &str) -> String {
    branch
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
