//! Updates without code signing: checks GitHub for a newer release on launch
//! and every 12 hours, offers it in the popover, the tray menu and one
//! notification, and updates by opening a terminal that runs the one-command
//! installer. The installer downloads and verifies the release, quits this
//! app, replaces it and opens the new one. When no terminal opens, Settings
//! shows the command to paste, already copied.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use everyport::update::{self, Install};
use semver::Version;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{launch, notify, settings, tray};

const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);
/// How often the loop looks at the clock. A timer alone would stall while the
/// computer sleeps.
const TICK: Duration = Duration::from_secs(10 * 60);

/// What Settings, the popover and the tray show.
#[derive(Clone, Serialize)]
pub struct Status {
    current: String,
    /// The newest release, once a check has found it.
    latest: Option<String>,
    /// `latest` when it's newer than this app.
    available: Option<String>,
    /// `available` unless the user skipped it: what the popover, tray and
    /// notification offer.
    offer: Option<String>,
    checking: bool,
    /// Why the last check failed.
    error: Option<String>,
    /// What the user has to do, when the update can't run on its own.
    manual: Option<Manual>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Manual {
    /// No terminal opened. The command is copied too, where the system has a clipboard tool.
    Paste { command: String },
    /// A system package (.deb, .rpm or .msi): install the new one from the release page.
    Package { url: String },
}

struct State {
    status: Status,
    checked_at: Option<SystemTime>,
    /// A terminal just started the installer, which quits this app.
    installing: bool,
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, State> {
    app.state::<Mutex<State>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Changes the status and shows it everywhere.
fn set(app: &AppHandle, change: impl FnOnce(&mut Status)) {
    let status = {
        let mut state = state(app);
        change(&mut state.status);
        state.status.clone()
    };
    tray::set_update(app, status.offer.as_deref());
    let _ = app.emit("updater", status);
}

pub fn setup(app: &AppHandle) {
    app.manage(Mutex::new(State {
        status: Status {
            current: app.package_info().version.to_string(),
            latest: None,
            available: None,
            offer: None,
            checking: false,
            error: None,
            manual: None,
        },
        checked_at: None,
        installing: false,
    }));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let due = state(&app)
                .checked_at
                .is_none_or(|at| at.elapsed().is_ok_and(|age| age >= CHECK_EVERY));
            if due && settings::app_settings(&app).check_updates {
                check(&app, false).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// Asks GitHub for the newest release. An automatic check notifies once per
/// version it offers.
async fn check(app: &AppHandle, manual: bool) {
    set(app, |status| status.checking = true);
    let found = update::latest().await;
    state(app).checked_at = Some(SystemTime::now());
    let prefs = settings::app_settings(app);
    let skipped = prefs
        .skip_version
        .as_deref()
        .and_then(|v| Version::parse(v).ok());
    set(app, |status| {
        status.checking = false;
        match &found {
            Ok(latest) => {
                let current = &app.package_info().version;
                let newer =
                    |skipped| update::offer(current, latest, skipped).map(|v| v.to_string());
                status.available = newer(None);
                status.offer = newer(skipped.as_ref());
                status.latest = Some(latest.to_string());
                status.error = None;
            }
            Err(error) => status.error = Some(format!("{error:#}")),
        }
    });
    let offer = state(app).status.offer.clone();
    if let Some(version) = offer.filter(|v| !manual && prefs.notified_version.as_ref() != Some(v)) {
        notify::show_update(app, version.clone());
        let saved = settings::edit_app(app, |prefs| prefs.notified_version = Some(version));
        if let Err(error) = saved {
            eprintln!("updater: {error}");
        }
    }
}

/// The tray item: installs the offered update, or checks for one.
pub fn from_menu(app: &AppHandle) {
    if state(app).status.offer.is_some() {
        install(app);
    } else {
        check_now(app);
    }
}

/// Opens Settings on General, where the result shows, and checks now.
fn check_now(app: &AppHandle) {
    settings::open(app, Some("General".into()));
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app, true).await });
}

#[tauri::command]
pub fn updater_status(app: AppHandle) -> Status {
    state(&app).status.clone()
}

#[tauri::command]
pub fn updater_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move { check(&app, true).await });
}

/// Hides this version's offer until a newer one comes out.
#[tauri::command]
pub fn updater_skip(app: AppHandle) -> Result<(), String> {
    let Some(version) = state(&app).status.available.clone() else {
        return Ok(());
    };
    settings::edit_app(&app, |prefs| prefs.skip_version = Some(version))?;
    set(&app, |status| status.offer = None);
    Ok(())
}

#[tauri::command]
pub async fn updater_install(app: AppHandle) {
    install(&app);
}

/// Updates the way this copy was installed, once. Off the calling thread,
/// since opening a terminal waits to see it start.
pub fn install(app: &AppHandle) {
    let Some(version) = state(app).status.available.clone() else {
        return;
    };
    if std::mem::replace(&mut state(app).installing, true) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let exe = std::env::current_exe().unwrap_or_default();
        match how(&exe) {
            How::Package => {
                state(&app).installing = false;
                let url = format!("{}/tag/v{version}", update::RELEASES);
                let _ = launch::open_external(app.clone(), url.clone());
                set(&app, |status| status.manual = Some(Manual::Package { url }));
                settings::open(&app, Some("General".into()));
            }
            How::Terminal { line, quit } => match open_terminal(&line) {
                Ok(()) => {
                    set(&app, |status| status.manual = None);
                    if quit {
                        app.exit(0);
                    }
                    // Allows another try if the installer failed.
                    std::thread::sleep(Duration::from_secs(60));
                    state(&app).installing = false;
                }
                Err(error) => {
                    eprintln!("updater: no terminal opened: {error}");
                    state(&app).installing = false;
                    copy(&line);
                    set(&app, |status| {
                        status.manual = Some(Manual::Paste { command: line })
                    });
                    settings::open(&app, Some("General".into()));
                }
            },
        }
    });
}

#[derive(Debug, PartialEq)]
enum How {
    /// Run `line` in a terminal, and quit first when it doesn't quit the app itself.
    Terminal { line: String, quit: bool },
    /// Installed from a system package, which only the package's installer updates.
    Package,
}

/// How to update the app running from `exe`.
fn how(exe: &Path) -> How {
    let script = |app_dir: Option<&Path>| {
        let install = Install::app();
        let install = match app_dir {
            Some(dir) => install.with("EVERYPORT_APP_DIR", dir.to_string_lossy()),
            None => install,
        };
        How::Terminal {
            line: install.line(),
            quit: false,
        }
    };
    if cfg!(target_os = "macos") {
        // .../Everyport.app/Contents/MacOS/everyport-desktop
        let bundle = exe
            .ancestors()
            .nth(3)
            .filter(|b| b.extension().is_some_and(|e| e == "app"));
        // The cask installs /Applications/Everyport.app.
        let cask = bundle == Some(Path::new("/Applications/Everyport.app"))
            && ["/opt/homebrew", "/usr/local"]
                .iter()
                .any(|prefix| Path::new(prefix).join("Caskroom/everyport").exists());
        if cask {
            // Homebrew replaces the app but leaves it running, so this quits
            // and the command opens the new one.
            return How::Terminal {
                line: "brew upgrade --cask everyport; open -a Everyport".into(),
                quit: true,
            };
        }
        let folder = bundle.and_then(Path::parent);
        script(folder.filter(|f| *f != Path::new("/Applications")))
    } else if cfg!(windows) {
        // The setup.exe leaves its uninstall.exe next to the app; the .msi doesn't.
        let folder = exe.parent().filter(|f| f.join("uninstall.exe").exists());
        let default = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Everyport"));
        match folder {
            Some(folder) if Some(folder) == default.as_deref() => script(None),
            Some(folder) => script(Some(folder)),
            None => How::Package,
        }
    } else if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
        let default = data.map(|d| d.join("everyport"));
        let folder = appimage.parent();
        script(folder.filter(|f| default.as_deref() != Some(*f)))
    } else if exe.starts_with("/usr") {
        How::Package
    } else {
        script(None)
    }
}

/// Opens a Terminal window that runs `line`. Terminal runs a .command file
/// it's asked to open, with no permission prompt, unlike scripting it with
/// osascript.
#[cfg(target_os = "macos")]
fn open_terminal(line: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("everyport-update.command");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nclear\necho 'Updating Everyport'\n{line}\n"),
    )?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    let status = std::process::Command::new("open")
        .args(["-a", "Terminal"])
        .arg(&script)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("open exited with {status}")))
    }
}

/// Opens the first terminal that starts, running `line`.
#[cfg(not(target_os = "macos"))]
fn open_terminal(line: &str) -> std::io::Result<()> {
    let mut last = std::io::Error::other("no terminal found");
    for argv in terminals(line) {
        let mut command = std::process::Command::new(&argv[0]);
        command.args(&argv[1..]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x10;
            command.creation_flags(CREATE_NEW_CONSOLE);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                last = error;
                continue;
            }
        };
        // A terminal that can't reach the desktop fails within a moment.
        // Some launchers hand off to a server and exit 0 instead.
        std::thread::sleep(std::time::Duration::from_millis(700));
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                last = std::io::Error::other(format!("{} exited with {status}", argv[0]));
            }
            Ok(Some(_)) => return Ok(()),
            _ => {
                // Still running: reap it when it closes.
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
        }
    }
    Err(last)
}

/// Puts `text` on the clipboard with the system's own tool, where there is one.
fn copy(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let tools: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(windows) {
        &[&["clip.exe"]]
    } else {
        &[&["wl-copy"], &["xclip", "-selection", "clipboard"]]
    };
    for argv in tools {
        let Ok(mut child) = Command::new(argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        let written = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(text.as_bytes()));
        if child.wait().is_ok_and(|status| status.success()) && matches!(written, Some(Ok(()))) {
            return;
        }
    }
}

/// The terminals to try, in order, each running `line`.
#[cfg(windows)]
fn terminals(line: &str) -> Vec<Vec<String>> {
    let powershell = |line: &str| {
        ["powershell.exe", "-NoExit", "-Command", line]
            .map(String::from)
            .to_vec()
    };
    // Windows Terminal splits its own command line at each `;` unless escaped.
    let wt = [
        vec!["wt.exe".to_string()],
        powershell(&line.replace(';', r"\;")),
    ]
    .concat();
    vec![wt, powershell(line)]
}

/// The terminals to try, in order, each running `line` and then a shell, so
/// the window stays open.
#[cfg(all(unix, not(target_os = "macos")))]
fn terminals(line: &str) -> Vec<Vec<String>> {
    let script = format!("{line}; exec \"${{SHELL:-sh}}\"");
    [
        ("x-terminal-emulator", "-e"),
        ("gnome-terminal", "--"),
        ("konsole", "-e"),
        ("xterm", "-e"),
    ]
    .into_iter()
    .map(|(terminal, run)| {
        [terminal, run, "sh", "-c", &script]
            .map(String::from)
            .to_vec()
    })
    .collect()
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_runs_the_installer_in_windows_terminal_or_powershell() {
        let line = "irm https://everyport.dev/install.ps1 | iex";
        assert_eq!(
            terminals(line),
            [
                vec!["wt.exe", "powershell.exe", "-NoExit", "-Command", line],
                vec!["powershell.exe", "-NoExit", "-Command", line],
            ]
        );
        let with_dir =
            r"$env:EVERYPORT_APP_DIR='D:\Apps'; irm https://everyport.dev/install.ps1 | iex";
        assert_eq!(
            terminals(with_dir)[0][4],
            r"$env:EVERYPORT_APP_DIR='D:\Apps'\; irm https://everyport.dev/install.ps1 | iex"
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_runs_the_installer_in_the_first_terminal_it_finds() {
        let line = "curl -fsSL https://everyport.dev/install.sh | sh";
        let script = r#"curl -fsSL https://everyport.dev/install.sh | sh; exec "${SHELL:-sh}""#;
        assert_eq!(
            terminals(line),
            [
                vec!["x-terminal-emulator", "-e", "sh", "-c", script],
                vec!["gnome-terminal", "--", "sh", "-c", script],
                vec!["konsole", "-e", "sh", "-c", script],
                vec!["xterm", "-e", "sh", "-c", script],
            ]
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_system_package_updates_from_the_release_page() {
        assert_eq!(how(Path::new("/usr/bin/everyport-desktop")), How::Package);
    }
}
