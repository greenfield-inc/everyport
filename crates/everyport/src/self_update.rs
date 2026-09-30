//! `everyport update`, and the note under `everyport --version` when a newer
//! release exists.

use crate::machine::RUNTIME;
use everyport::update::{self, Install};
use semver::Version;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime};

/// `--version` checks GitHub at most this often.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// After a failed check, `--version` waits this long before trying again.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

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
    remember(Some(&latest));
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
    // Run from memory, with no file anyone else could swap: on stdin for sh,
    // and encoded as PowerShell expects for -EncodedCommand.
    let mut command = if cfg!(windows) {
        use base64::Engine;
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                // Plain text even when the output is captured, not CLIXML.
                "-OutputFormat",
                "Text",
                "-EncodedCommand",
            ])
            .arg(base64::engine::general_purpose::STANDARD.encode(utf16));
        command
    } else {
        let mut command = Command::new("sh");
        command.arg("-s").stdin(Stdio::piped());
        command
    };
    let mut child = command.envs(install.env.iter().cloned()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script.as_bytes())?;
    }
    Ok(if child.wait()?.success() {
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
/// GitHub at most once a day, an hour after a failed check, never when
/// `EVERYPORT_NO_UPDATE_CHECK` is set, and never for scripts reading the output.
pub fn note_after_version() {
    if std::env::var_os("EVERYPORT_NO_UPDATE_CHECK").is_some_and(|v| v != "0" && !v.is_empty())
        || !io::stdout().is_terminal()
    {
        return;
    }
    let within = |at: SystemTime, limit| at.elapsed().is_ok_and(|age| age < limit);
    let latest = match remembered() {
        Some((at, Some(latest))) if within(at, CHECK_EVERY) => latest,
        Some((at, None)) if within(at, RETRY_AFTER) => return,
        _ => match RUNTIME.block_on(update::latest()) {
            Ok(latest) => {
                remember(Some(&latest));
                latest
            }
            // Offline, or GitHub is slow: say nothing, and wait an hour.
            Err(_) => {
                remember(None);
                return;
            }
        },
    };
    if let Some(newer) = update::offer(&update::current(), &latest, None) {
        eprintln!("everyport {newer} is out. Run `everyport update` to update.");
    }
}

/// Where the last check's result is kept between runs: the newest
/// release's version, or nothing when it failed. Its time is the file's.
fn cache_file() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("everyport").join("latest-release"))
}

/// When the last check ran, and the version it found.
fn remembered() -> Option<(SystemTime, Option<Version>)> {
    let path = cache_file()?;
    let checked = path.metadata().ok()?.modified().ok()?;
    let latest = Version::parse(std::fs::read_to_string(path).ok()?.trim()).ok();
    Some((checked, latest))
}

fn remember(latest: Option<&Version>) {
    if let Some(path) = cache_file() {
        let _ = std::fs::create_dir_all(path.parent().expect("the cache file is in a folder"));
        let _ = std::fs::write(path, latest.map(Version::to_string).unwrap_or_default());
    }
}
