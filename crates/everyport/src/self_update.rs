//! `everyport update`, and the note under `everyport --version` when a newer
//! release exists.

use crate::machine::RUNTIME;
use everyport::update::{self, Install};
use semver::Version;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

/// `--version` checks GitHub at most this often.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Updates this everyport in place with the CLI installer. A copy that a
/// package manager put here gets that package manager's command instead.
pub fn run() -> io::Result<ExitCode> {
    let exe = std::env::current_exe()?;
    if let Some(how) = package_manager(&exe.canonicalize()?) {
        println!("This everyport came from {how}");
        return Ok(ExitCode::SUCCESS);
    }
    let current = update::current();
    let latest = RUNTIME
        .block_on(update::latest())
        .map_err(|e| io::Error::other(format!("{e:#}")))?;
    remember(&latest);
    if update::offer(&current, &latest, None).is_none() {
        println!("everyport {current} is the latest version.");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Updating everyport {current} to {latest}");
    let dir = exe.parent().expect("an executable is in a folder");
    let install = Install::cli().with("EVERYPORT_INSTALL_DIR", dir.to_string_lossy());
    // Downloaded first, so a failed download fails the update.
    let script = RUNTIME
        .block_on(update::fetch(&install.url))
        .map_err(|e| io::Error::other(format!("{e:#}")))?;
    let extension = if cfg!(windows) { "ps1" } else { "sh" };
    let path = std::env::temp_dir().join(format!(
        "everyport-install-{}.{extension}",
        std::process::id()
    ));
    std::fs::write(&path, script)?;
    let mut command = if cfg!(windows) {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        command
    } else {
        Command::new("sh")
    };
    let status = command
        .arg(&path)
        .envs(install.env.iter().cloned())
        .status();
    let _ = std::fs::remove_file(&path);
    Ok(if status?.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// How to update a copy a package manager owns, from where it lives.
fn package_manager(exe: &Path) -> Option<String> {
    let path = exe.to_string_lossy().replace('\\', "/");
    let version = update::current().to_string();
    if path.contains("/Cellar/") {
        Some("Homebrew. Update it with: brew upgrade everyport".into())
    } else if path.contains("/.cargo/bin/") {
        Some("cargo. Update it with: cargo install everyport".into())
    } else if path.contains(&format!("/everyport/{version}/")) {
        // The npm and PyPI packages keep the binary in a cache folder per version.
        Some("npm or PyPI. Update the package: npm install -g everyport, or pip install -U everyport. npx and uvx use the latest on their own.".into())
    } else {
        None
    }
}

/// After `--version` in a terminal, says when a newer release exists. Checks
/// GitHub at most once a day, never when `EVERYPORT_NO_UPDATE_CHECK` is set,
/// and never for scripts reading the output.
pub fn note_after_version() {
    if std::env::var_os("EVERYPORT_NO_UPDATE_CHECK").is_some_and(|v| v != "0" && !v.is_empty())
        || !io::stdout().is_terminal()
    {
        return;
    }
    let latest = match remembered() {
        Some((checked, latest)) if checked.elapsed().is_ok_and(|age| age < CHECK_EVERY) => latest,
        _ => match RUNTIME.block_on(update::latest()) {
            Ok(latest) => {
                remember(&latest);
                latest
            }
            // Offline, or GitHub is slow: say nothing, and try again next time.
            Err(_) => return,
        },
    };
    if let Some(newer) = update::offer(&update::current(), &latest, None) {
        eprintln!("everyport {newer} is out. Run `everyport update` to update.");
    }
}

/// Where the newest release's version is kept between runs.
fn cache_file() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("everyport").join("latest-release"))
}

/// The version the last check found, and when it ran.
fn remembered() -> Option<(SystemTime, Version)> {
    let path = cache_file()?;
    let checked = path.metadata().ok()?.modified().ok()?;
    let latest = Version::parse(std::fs::read_to_string(path).ok()?.trim()).ok()?;
    Some((checked, latest))
}

fn remember(latest: &Version) {
    if let Some(path) = cache_file() {
        let _ = std::fs::create_dir_all(path.parent().expect("the cache file is in a folder"));
        let _ = std::fs::write(path, latest.to_string());
    }
}
