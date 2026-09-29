//! Onboarding: a window of short steps that opens once, on first launch
//! (see `settings::setup`). The page is `Onboarding` from @everyport/ui; this
//! side answers its checks: which agents, workspaces and editors this
//! computer has, and whether the `everyport` command is installed.

use std::path::{Path, PathBuf};

use everyport::client::install;
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::launch::EDITORS;
use crate::popover;

pub const LABEL: &str = "onboarding";

pub fn open(app: &AppHandle) {
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => match WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
            .title("Welcome to Everyport")
            .inner_size(480.0, 620.0)
            .resizable(false)
            .maximizable(false)
            .center()
            .build()
        {
            Ok(window) => window,
            Err(error) => return eprintln!("onboarding: {error}"),
        },
    };
    // An accessory app has to activate itself to bring a window forward.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Closes onboarding and opens the popover next to the tray icon.
#[tauri::command]
pub fn onboarding_finish(app: AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
    popover::show(&app);
}

/// A tool the tools step looks for. `found` says where, such as `~/.claude`.
#[derive(Serialize)]
pub struct Tool {
    name: &'static str,
    kind: &'static str,
    found: Option<String>,
}

#[tauri::command]
pub async fn onboarding_tools() -> Vec<Tool> {
    tauri::async_runtime::spawn_blocking(tools)
        .await
        .unwrap_or_default()
}

fn tools() -> Vec<Tool> {
    let home = home();
    let agents = everyport::links::AgentDirs::from_env();
    let pane = std::env::var_os("PANE_DIR")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".pane")));
    let dir =
        |path: Option<PathBuf>, shown: &str| path.filter(|p| p.is_dir()).map(|_| shown.to_string());
    let installed = |found: bool| found.then(|| "Installed".to_string());
    let mut list = vec![
        Tool {
            name: "Claude Code",
            kind: "claude_code",
            found: dir(agents.claude, "~/.claude"),
        },
        Tool {
            name: "Codex",
            kind: "codex",
            found: dir(agents.codex, "~/.codex"),
        },
    ];
    if cfg!(target_os = "macos") {
        let support = home
            .as_ref()
            .map(|h| h.join("Library/Application Support/com.conductor.app"));
        list.push(Tool {
            name: "Conductor",
            kind: "conductor",
            found: installed(has_app("com.conductor.app") || support.is_some_and(|p| p.is_dir())),
        });
    }
    list.push(Tool {
        name: "Pane",
        kind: "pane",
        found: dir(pane, "~/.pane").or_else(|| installed(has_app("com.dcouple.pane"))),
    });
    list.push(Tool {
        name: "Editor",
        kind: "editor",
        found: EDITORS
            .iter()
            .find(|editor| {
                if cfg!(target_os = "macos") {
                    has_app(editor.bundle_id)
                } else {
                    on_path(editor.command).is_some()
                }
            })
            .map(|editor| editor.name.to_string()),
    });
    list.push(Tool {
        name: "GitHub CLI",
        kind: "gh",
        found: on_path(if cfg!(windows) { "gh.exe" } else { "gh" }).map(|_| "gh".into()),
    });
    list
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Whether macOS knows an app with this bundle id, wherever it's installed.
#[cfg(target_os = "macos")]
fn has_app(bundle_id: &str) -> bool {
    use tauri_nspanel::objc2_app_kit::NSWorkspace;
    use tauri_nspanel::objc2_foundation::NSString;
    NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))
        .is_some()
}

#[cfg(not(target_os = "macos"))]
fn has_app(_bundle_id: &str) -> bool {
    false
}

/// Where `file` is on `PATH`, or in Homebrew's folders, which an app opened
/// from the Dock may not have on its `PATH`.
fn on_path(file: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
        .map(|dir| dir.join(file))
        .find(|file| file.is_file())
}

/// The `everyport` command on this computer, as the terminal step shows it.
#[derive(Serialize)]
pub struct Cli {
    /// Where it is, or where Install puts it.
    path: String,
    installed: bool,
    /// Set when it's in a folder that terminals may not search.
    hint: Option<String>,
}

fn tilde(path: &Path) -> String {
    match home() {
        Some(home) => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
}

/// Finds `everyport` where the probe that checks other machines looks (the
/// install folder, then `PATH`), and where Homebrew puts it.
#[tauri::command]
pub async fn cli_status() -> Result<Cli, String> {
    let probe = install::probe(&[]).await.map_err(|e| format!("{e:#}"))?;
    let ours = PathBuf::from(&probe.install_path);
    let found = match probe.installed {
        Some(_) if probe.everyport_path == probe.install_path => Some(ours.clone()),
        Some(_) => Some(PathBuf::from(&probe.everyport_path)),
        None => on_path(if cfg!(windows) {
            "everyport.exe"
        } else {
            "everyport"
        }),
    };
    let dir = ours.parent().map(tilde).unwrap_or_default();
    Ok(Cli {
        path: tilde(found.as_deref().unwrap_or(&ours)),
        installed: found.is_some(),
        hint: (found.as_ref() == Some(&ours))
            .then(|| format!("If your terminal can't find it, add {dir} to your PATH.")),
    })
}

/// Installs `everyport` with the code that installs it on other machines,
/// one install at a time.
#[tauri::command]
pub async fn cli_install() -> Result<Cli, String> {
    static INSTALLING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = INSTALLING.lock().await;
    let probe = install::probe(&[]).await.map_err(|e| format!("{e:#}"))?;
    if probe.installed.is_none() {
        install::install(&[], &probe)
            .await
            .map_err(|e| format!("{e:#}"))?;
    }
    cli_status().await
}
