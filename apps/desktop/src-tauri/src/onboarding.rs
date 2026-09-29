//! Onboarding: a window of short steps that opens once, on first launch
//! (see `settings::setup`). The page is `Onboarding` from @everyport/ui; this
//! side answers its checks: which agents, workspaces and editors this
//! computer has, and whether the `everyport` command is installed.

use std::path::{Path, PathBuf};

use everyport::client::install;
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

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
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let agents = everyport::links::AgentDirs::from_env();
    let dir =
        |path: Option<PathBuf>, shown: &str| path.filter(|p| p.is_dir()).map(|_| shown.to_string());
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
            found: (mac_app(home.as_deref(), "Conductor") || support.is_some_and(|p| p.is_dir()))
                .then(|| "Installed".into()),
        });
    }
    list.push(Tool {
        name: "Pane",
        kind: "pane",
        found: dir(home.as_ref().map(|h| h.join(".pane")), "~/.pane")
            .or_else(|| mac_app(home.as_deref(), "Pane").then(|| "Installed".into())),
    });
    list.push(Tool {
        name: "Editor",
        kind: "editor",
        found: editor(home.as_deref()).map(String::from),
    });
    list.push(Tool {
        name: "GitHub CLI",
        kind: "gh",
        found: on_path(if cfg!(windows) { "gh.exe" } else { "gh" }).then(|| "gh".into()),
    });
    list
}

/// The first editor `launch::open_in_editor` would use.
fn editor(home: Option<&Path>) -> Option<&'static str> {
    let editors: &[(&str, &str, &str)] = &[
        ("Cursor", "Cursor", "cursor"),
        ("VS Code", "Visual Studio Code", "code"),
        ("Zed", "Zed", "zed"),
        ("Sublime Text", "Sublime Text", "subl"),
    ];
    editors.iter().find_map(|&(name, app, command)| {
        let found = if cfg!(target_os = "macos") {
            mac_app(home, app)
        } else if cfg!(windows) {
            on_path(&format!("{command}.cmd")) || on_path(&format!("{command}.exe"))
        } else {
            on_path(command)
        };
        found.then_some(name)
    })
}

/// An app bundle in /Applications or ~/Applications.
fn mac_app(home: Option<&Path>, name: &str) -> bool {
    let bundle = format!("{name}.app");
    Path::new("/Applications").join(&bundle).exists()
        || home.is_some_and(|h| h.join("Applications").join(&bundle).exists())
}

/// On `PATH`, or in Homebrew's folders, which an app opened from the Dock may
/// not have on its `PATH`.
fn on_path(file: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
        .any(|dir| dir.join(file).is_file())
}

/// The `everyport` command on this computer, as the terminal step shows it.
#[derive(Serialize)]
pub struct Cli {
    path: String,
    installed: Option<String>,
    version: &'static str,
}

impl From<install::Probe> for Cli {
    fn from(probe: install::Probe) -> Self {
        Cli {
            path: tilde(&probe.install_path),
            installed: probe.installed,
            version: install::VERSION,
        }
    }
}

fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

/// Checks this computer the way Settings → Machines checks another one: the
/// same probe, through no command prefix.
#[tauri::command]
pub async fn cli_status() -> Result<Cli, String> {
    install::probe(&[])
        .await
        .map(Cli::from)
        .map_err(|e| format!("{e:#}"))
}

/// Installs the `everyport` command with the code that installs it on other machines.
#[tauri::command]
pub async fn cli_install() -> Result<Cli, String> {
    let probe = install::probe(&[]).await.map_err(|e| format!("{e:#}"))?;
    install::install(&[], &probe)
        .await
        .map_err(|e| format!("{e:#}"))?;
    cli_status().await
}
