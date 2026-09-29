//! Human formats shared by the list, clean up and the TUI, following the popover.

use everyport_core::protocol::{AgentKind, CleanUpReason, OtherPort, Server};

/// Read-only listener details when the owning process cannot be inspected.
pub fn other_port(port: &OtherPort) -> String {
    let mut fields = vec![format!(":{}", port.port), port.addresses.join(", ")];
    if let Some(owner) = &port.owner {
        fields.push(owner.clone());
    }
    if let Some(name) = &port.process_name {
        fields.push(name.clone());
    }
    fields.join("  ")
}

const MB: f64 = 1_048_576.0;

/// "612 MB" or "1.25 GB", in binary units like Activity Monitor.
pub fn bytes(value: u64) -> String {
    let mb = value as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Compact total for headers: "4.9 GB", "16 GB" or "612 MB".
pub fn total(value: u64) -> String {
    let mb = value as f64 / MB;
    if mb >= 1024.0 {
        let text = format!("{:.1}", mb / 1024.0);
        format!("{} GB", text.strip_suffix(".0").unwrap_or(&text))
    } else {
        format!("{mb:.0} MB")
    }
}

/// "<1m", "42m", "3h 5m" or "2d".
pub fn duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    match minutes {
        0 => "<1m".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 if minutes.is_multiple_of(60) => format!("{}h", minutes / 60),
        60..=1439 => format!("{}h {}m", minutes / 60, minutes % 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// "5m", "3h" or "2d".
pub fn short_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    match minutes {
        0..=59 => format!("{}m", minutes.max(1)),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// Whole numbers from 10% up, one decimal below, so a quiet server reads "0.3%".
pub fn percent(value: f32) -> String {
    if value <= 0.0 {
        "0%".into()
    } else if value < 0.1 {
        "<0.1%".into()
    } else if value < 10.0 {
        format!("{value:.1}%")
    } else {
        format!("{}%", value.round() as u64)
    }
}

/// "1 server", "3 servers".
pub fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub fn reason(reason: &CleanUpReason) -> String {
    match reason {
        CleanUpReason::WorktreeDeleted => "Worktree deleted".into(),
        CleanUpReason::Idle { seconds } => {
            format!("Idle {} · no connections", short_duration(*seconds))
        }
        CleanUpReason::LongRunning { seconds } => {
            format!("Running for {}", short_duration(*seconds))
        }
        CleanUpReason::Leaking { bytes: growth } => format!("Leaking · +{}", bytes(*growth)),
    }
}

pub fn agent(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::ClaudeCode => "Claude Code",
        AgentKind::Codex => "Codex",
    }
}

/// Seconds since the server started, as of `now` (Unix ms).
pub fn uptime(server: &Server, now: u64) -> Option<u64> {
    server.started_at.map(|at| now.saturating_sub(at) / 1000)
}

pub fn idle(server: &Server, now: u64) -> u64 {
    now.saturating_sub(server.last_active) / 1000
}

/// Clean up suggests these. Leaking servers are listed but left unchecked,
/// since they're usually still in use.
pub fn preselected(server: &Server) -> bool {
    !server.protected
        && matches!(server.clean_up, Some(reason) if !matches!(reason, CleanUpReason::Leaking { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_match_the_popover() {
        assert_eq!(bytes(642 * 1_048_576), "642 MB");
        assert_eq!(bytes(2_684_354_560), "2.50 GB");
        assert_eq!(total(17_179_869_184), "16 GB");
        assert_eq!(total(5_261_334_938), "4.9 GB");
        assert_eq!(duration(30), "<1m");
        assert_eq!(duration(3 * 3600 + 5 * 60), "3h 5m");
        assert_eq!(duration(7200), "2h");
        assert_eq!(duration(3 * 86400), "3d");
        assert_eq!(short_duration(20), "1m");
        assert_eq!(short_duration(5 * 3600 + 59 * 60), "5h");
        assert_eq!(percent(0.05), "<0.1%");
        assert_eq!(percent(0.34), "0.3%");
        assert_eq!(percent(12.6), "13%");
    }
}
