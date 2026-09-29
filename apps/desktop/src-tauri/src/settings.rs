//! Settings: the window, and the settings it edits. Scanner settings live in
//! `config.toml`, which every `everyport` command also reads; the app's own (theme,
//! appearance, shortcut) live next to it in `app.toml`. Changes apply at once,
//! from Settings or from an edit to either file: the scanner through
//! `Call::Configure` (see `change_for`), the rest through a `settings` event
//! to every window.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use everyport::client::machines as saved;
use everyport::client::machines::Via;
use everyport::client::{discover, Connection};
use everyport::config;
use everyport::protocol::{Call, Config, ConfigChange};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{machines, onboarding, popover, tray};

pub const LABEL: &str = "settings";

/// The app's own settings, in `app.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct App {
    /// A Doozy theme name; the page's default when unset.
    pub theme: Option<String>,
    /// `system`, `light` or `dark`.
    pub appearance: String,
    /// Opens the popover from anywhere, in Tauri's accelerator syntax.
    pub shortcut: String,
    /// Onboarding has opened, and launch at login was turned on with it. Set
    /// once, so neither happens again, even after the user turns it off.
    pub onboarded: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            theme: None,
            appearance: "system".into(),
            shortcut: "CommandOrControl+Alt+P".into(),
            onboarded: false,
        }
    }
}

/// Everything the Settings window shows.
#[derive(Clone, Serialize)]
pub struct Settings {
    pub config: Config,
    pub app: App,
    /// Why `config.toml` can't be read. Nothing saves or sends scanner
    /// settings until it's fixed, so a typo never wipes the protected list.
    pub config_error: Option<String>,
    /// Why `app.toml` can't be read. Its settings don't save until it's fixed.
    pub app_error: Option<String>,
}

fn config_file() -> Result<PathBuf, String> {
    config::path().ok_or_else(|| "no config folder for this user".into())
}

fn app_file() -> Result<PathBuf, String> {
    Ok(config::dir()
        .ok_or("no config folder for this user")?
        .join("app.toml"))
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, Settings> {
    app.state::<Mutex<Settings>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Reads both files, registers the shortcut, and reloads the files when
/// they change, such as after an edit by hand. On first launch also turns on
/// launch at login and opens onboarding.
pub fn setup(app: &AppHandle) {
    app.manage(Mutex::new(Pane(None)));
    app.manage(Mutex::new(Settings {
        config: Config::default(),
        app: App::default(),
        config_error: None,
        app_error: None,
    }));
    // The default first; `reload` moves it to the saved one.
    let default = App::default().shortcut;
    if let Err(error) = register(app, &default) {
        eprintln!("shortcut {default}: {error}");
    }
    let first = app_file().and_then(|path| first_run(&path));
    reload(app);
    match watch(app) {
        Ok(watcher) => {
            app.manage(watcher);
        }
        Err(error) => eprintln!("settings: can't watch the config folder: {error}"),
    }
    match first {
        Ok(true) => {
            if let Err(error) = set_launch_at_login(app.clone(), true) {
                eprintln!("launch at login: {error}");
            }
            onboarding::open(app);
        }
        Ok(false) => {}
        Err(error) => eprintln!("first run: {error}"),
    }
}

/// Whether this is the first launch, which it records in `app.toml`. Every
/// earlier version made the config folder on launch, so an existing folder
/// without the record is an upgrade, whose launch at login stays as the user
/// left it. An unreadable file is never overwritten.
fn first_run(path: &Path) -> Result<bool, String> {
    let upgrade = path.parent().is_some_and(Path::exists);
    let mut prefs = load_app(path)?;
    if prefs.onboarded {
        return Ok(false);
    }
    prefs.onboarded = true;
    save_app(path, &prefs)?;
    Ok(!upgrade)
}

fn watch(app: &AppHandle) -> Result<notify::RecommendedWatcher, String> {
    use notify::{RecursiveMode, Watcher};
    let dir = config::dir().ok_or("no config folder for this user")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (config, prefs) = (config_file()?, app_file()?);
    let app = app.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|e| e.paths.iter().any(|p| *p == config || *p == prefs)) {
            reload(&app);
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    Ok(watcher)
}

/// Reads both files again. Scanner settings that changed go to every
/// machine; a file that can't be read keeps what was in use and says why.
fn reload(app: &AppHandle) {
    let config = config_file().and_then(|path| config::load(&path).map_err(|e| e.to_string()));
    let prefs = app_file().and_then(|path| load_app(&path));
    let changed = {
        let mut settings = state(app);
        let changed = match config {
            Ok(config) => {
                let changed = config != settings.config || settings.config_error.is_some();
                settings.config = config;
                settings.config_error = None;
                changed
            }
            Err(error) => {
                settings.config_error = Some(error);
                false
            }
        };
        match prefs {
            Ok(prefs) => {
                if prefs.shortcut != settings.app.shortcut
                    && switch_shortcut(app, &settings.app.shortcut, &prefs.shortcut).is_err()
                {
                    eprintln!("shortcut {}: can't be used", prefs.shortcut);
                }
                settings.app = prefs;
                settings.app_error = None;
            }
            Err(error) => settings.app_error = Some(error),
        }
        changed
    };
    publish(app);
    if changed {
        configure_all(app);
    }
}

/// The scanner settings for a machine as it connects, or none while
/// `config.toml` can't be read.
pub fn change_for(app: &AppHandle, machine_id: &str) -> Option<ConfigChange> {
    let settings = state(app);
    if settings.config_error.is_some() {
        return None;
    }
    Some(machine_change(&settings.config, machine_id))
}

/// What a machine takes from the user's settings. This computer gets all of
/// them. Other machines keep their own `config.toml`, auto-kill included,
/// and only follow the Vercel previews switch.
fn machine_change(config: &Config, machine_id: &str) -> ConfigChange {
    if machine_id == machines::LOCAL {
        config.clone().into()
    } else {
        ConfigChange {
            vercel_previews: Some(config.vercel_previews),
            ..ConfigChange::default()
        }
    }
}

fn configure_all(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for machine in machines::machines_list(app.clone()) {
            let Some(change) = change_for(&app, &machine.id) else {
                return;
            };
            if let Err(error) = machines::call(&app, &machine.id, Call::Configure(change)).await {
                eprintln!("configure {}: {error}", machine.id);
            }
        }
    });
}

fn load_app(path: &Path) -> Result<App, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e.message()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(App::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn save_app(path: &Path, prefs: &App) -> Result<(), String> {
    let text = toml::to_string(prefs).map_err(|e| e.to_string())?;
    config::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

fn publish(app: &AppHandle) {
    let settings = state(app).clone();
    let _ = app.emit("settings", settings);
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    state(&app).clone()
}

/// Saves the scanner settings and sends them to every machine.
#[tauri::command]
pub fn settings_set_config(app: AppHandle, config: Config) -> Result<(), String> {
    if let Some(error) = state(&app).config_error.clone() {
        return Err(format!("Fix config.toml first. {error}"));
    }
    let path = config_file()?;
    config::save(&path, &config).map_err(|e| format!("{}: {e}", path.display()))?;
    state(&app).config = config;
    publish(&app);
    configure_all(&app);
    Ok(())
}

/// Saves the app's settings. A new shortcut is registered first, and nothing
/// changes when it can't be.
#[tauri::command]
pub fn settings_set_app(app: AppHandle, prefs: App) -> Result<(), String> {
    let (old, error) = {
        let settings = state(&app);
        (settings.app.shortcut.clone(), settings.app_error.clone())
    };
    if let Some(error) = error {
        return Err(format!("Fix app.toml first. {error}"));
    }
    if prefs.shortcut != old {
        check_shortcut(&prefs.shortcut)?;
        switch_shortcut(&app, &old, &prefs.shortcut)?;
    }
    save_app(&app_file()?, &prefs)?;
    state(&app).app = prefs;
    publish(&app);
    Ok(())
}

/// Moves the global shortcut from `old` to `new`, or keeps `old` when `new`
/// can't be registered.
fn switch_shortcut(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    let _ = app.global_shortcut().unregister(old);
    register(app, new).inspect_err(|_| {
        let _ = register(app, old);
    })
}

fn register(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                popover::toggle(app);
            }
        })
        .map_err(|_| "Another app already uses this shortcut. Try a different one.".to_string())
}

/// Refuses shortcuts that would take a key from typing or from other apps'
/// own shortcuts, such as ⌘C or ⌘⇧K: a global shortcut needs ⌥/Alt, or both
/// ⌘ and Ctrl. On macOS it also needs ⌘ or ⌃, since ⌥ with a key types a
/// character there.
fn check_shortcut(shortcut: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::Modifiers;
    let mods = shortcut
        .parse::<Shortcut>()
        .map_err(|_| format!("{shortcut} isn't a shortcut"))?
        .mods;
    let primary = mods.intersects(Modifiers::SUPER | Modifiers::CONTROL);
    if cfg!(target_os = "macos") && !primary {
        return Err("Add ⌘ or ⌃. With only ⌥, the keys type a character.".into());
    }
    if mods.contains(Modifiers::ALT) || mods.contains(Modifiers::SUPER | Modifiers::CONTROL) {
        Ok(())
    } else if cfg!(target_os = "macos") {
        Err("Other apps use this shortcut. Add ⌥ to it.".into())
    } else {
        Err("Other apps use this shortcut. Add Alt to it.".into())
    }
}

/// The pane Settings should show next, taken by the page when it comes forward.
struct Pane(Option<String>);

/// Opens Settings, on `pane` when given.
pub fn open(app: &AppHandle, pane: Option<String>) {
    if pane.is_some() {
        app.state::<Mutex<Pane>>()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0 = pane;
    }
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => match WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
            .title("Settings")
            .inner_size(640.0, 520.0)
            .min_inner_size(560.0, 420.0)
            .center()
            .build()
        {
            Ok(window) => {
                let hide = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide.hide();
                    }
                });
                window
            }
            Err(error) => return eprintln!("settings: {error}"),
        },
    };
    // An accessory app has to activate itself to bring a window forward.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn open_settings(app: AppHandle, pane: Option<String>) {
    open(&app, pane);
}

/// The pane `open` asked for, once.
#[tauri::command]
pub fn settings_take_pane(app: AppHandle) -> Option<String> {
    app.state::<Mutex<Pane>>()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .0
        .take()
}

#[tauri::command]
pub fn launch_at_login(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    tray::sync_launch_at_login(&app);
    result.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- machines

/// Settings → Machines: the saved machines in `machines.toml`, and the ones
/// discovery found that aren't saved yet. Their status comes from `machines_list`.
#[derive(Serialize)]
pub struct MachineSettings {
    saved: Vec<Saved>,
    found: Vec<Found>,
}

#[derive(Serialize)]
struct Saved {
    name: String,
    /// The command prefix, or the `everyport serve` URL.
    target: String,
}

#[derive(Serialize)]
struct Found {
    name: String,
    source: &'static str,
    command: String,
}

fn machines_file() -> Result<PathBuf, String> {
    saved::path().ok_or_else(|| "no config folder for this user".into())
}

fn load_machines(path: &Path) -> Result<Vec<saved::Machine>, String> {
    saved::load(path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn settings_machines() -> Result<MachineSettings, String> {
    let list = load_machines(&machines_file()?)?;
    let found = discover::all()
        .await
        .into_iter()
        .filter(|f| !list.iter().any(|m| m.name == f.machine.name))
        .filter_map(|f| {
            let Via::Command { command } = f.machine.via else {
                return None;
            };
            Some(Found {
                name: f.machine.name,
                source: match f.source {
                    discover::Source::SshConfig => "SSH config",
                    discover::Source::Pane => "Pane",
                    discover::Source::Wsl => "WSL",
                },
                command: shell_words::join(&command),
            })
        })
        .collect();
    let saved = list
        .into_iter()
        .map(|m| Saved {
            target: match m.via {
                Via::Command { command } => shell_words::join(&command),
                Via::Url { url, .. } => url,
            },
            name: m.name,
        })
        .collect();
    Ok(MachineSettings { saved, found })
}

/// Saves a machine reached through `target`: a command prefix such as
/// `ssh devbox`, quoted like a shell command, or an `everyport://` code from
/// `everyport serve`.
#[tauri::command]
pub fn machine_add(app: AppHandle, name: String, target: String) -> Result<(), String> {
    add_machine(&machines_file()?, &name, &target)?;
    machines::reload(&app);
    Ok(())
}

#[tauri::command]
pub fn machine_remove(app: AppHandle, name: String) -> Result<(), String> {
    let path = machines_file()?;
    let mut list = load_machines(&path)?;
    list.retain(|m| m.name != name);
    saved::save(&path, &list).map_err(|e| format!("{e:#}"))?;
    machines::reload(&app);
    Ok(())
}

fn add_machine(path: &Path, name: &str, target: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the machine a name.".into());
    }
    if name == machines::LOCAL {
        return Err(format!("{name} is this computer's name. Pick another."));
    }
    let target = target.trim();
    let via = if target.starts_with("everyport://") {
        match Connection::from_code(target).map_err(|e| e.to_string())? {
            Connection::Http { url, token } => Via::Url { url, token },
            _ => unreachable!("a code is always an HTTP connection"),
        }
    } else {
        // Quoted like a shell command, but never run through one.
        let command = shell_words::split(target)
            .map_err(|_| "That command has an unclosed quote.".to_string())?;
        if command.is_empty() {
            return Err("Enter a command, such as ssh devbox, or an everyport:// code.".into());
        }
        Via::Command { command }
    };
    let mut list = load_machines(path)?;
    if list.iter().any(|m| m.name == name) {
        return Err(format!("There's already a machine named {name}."));
    }
    list.push(saved::Machine {
        name: name.into(),
        via,
    });
    saved::save(path, &list).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use everyport::protocol::AutoKill;

    #[test]
    fn adds_machines_by_command_or_code() {
        let dir = std::env::temp_dir().join(format!(
            "everyport-settings-machines-{}",
            std::process::id()
        ));
        let path = dir.join("machines.toml");
        add_machine(
            &path,
            " devbox ",
            r#"  ssh -i "/Users/me/My Keys/id" devbox "#,
        )
        .unwrap();
        // printf '{"url":"http://127.0.0.1:7767","token":"s3cret"}' | base64 | tr '+/' '-_' | tr -d '='
        let code = "everyport://eyJ1cmwiOiJodHRwOi8vMTI3LjAuMC4xOjc3NjciLCJ0b2tlbiI6InMzY3JldCJ9";
        add_machine(&path, "mini", code).unwrap();
        assert_eq!(
            load_machines(&path).unwrap(),
            [
                saved::Machine {
                    name: "devbox".into(),
                    via: Via::Command {
                        command: ["ssh", "-i", "/Users/me/My Keys/id", "devbox"]
                            .map(String::from)
                            .to_vec(),
                    },
                },
                saved::Machine {
                    name: "mini".into(),
                    via: Via::Url {
                        url: "http://127.0.0.1:7767".into(),
                        token: everyport::client::Token("s3cret".into()),
                    },
                },
            ]
        );
        assert!(add_machine(&path, "devbox", "ssh devbox").is_err());
        assert!(add_machine(&path, "local", "ssh devbox").is_err());
        assert!(add_machine(&path, "", "ssh devbox").is_err());
        assert!(add_machine(&path, "box", "  ").is_err());
        assert!(add_machine(&path, "box", "everyport://nope").is_err());
        assert!(add_machine(&path, "box", "ssh 'devbox").is_err());
        assert_eq!(load_machines(&path).unwrap().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_machines_get_only_the_vercel_switch() {
        let config = Config {
            auto_kill: AutoKill::Act,
            protected: vec!["caddy".into()],
            vercel_previews: true,
            ..Config::default()
        };
        let remote = machine_change(&config, "devbox");
        assert_eq!(
            remote,
            ConfigChange {
                vercel_previews: Some(true),
                ..ConfigChange::default()
            }
        );
        let local = machine_change(&config, machines::LOCAL);
        assert_eq!(local.auto_kill, Some(AutoKill::Act));
        assert_eq!(local.protected, Some(vec!["caddy".to_string()]));
    }

    #[test]
    fn keeps_shortcuts_that_other_apps_rarely_use() {
        for ok in [
            "CommandOrControl+Alt+P",
            "Super+Control+K",
            "Control+Alt+Shift+9",
        ] {
            assert_eq!(check_shortcut(ok), Ok(()), "{ok}");
        }
        // ⌥Space types a non-breaking space on a Mac; elsewhere Alt+Space is free.
        assert_eq!(
            check_shortcut("Alt+Space").is_ok(),
            !cfg!(target_os = "macos")
        );
    }

    #[test]
    fn refuses_shortcuts_that_take_keys_from_other_apps() {
        for taken in [
            "P",
            "Shift+P",
            "CommandOrControl+C",
            "Super+Shift+K",
            "Nonsense+P",
        ] {
            assert!(check_shortcut(taken).is_err(), "{taken}");
        }
    }

    #[test]
    fn launch_at_login_turns_on_once_and_never_again() {
        let dir = std::env::temp_dir().join(format!("everyport-first-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("app.toml");
        // First launch: no config folder yet.
        assert_eq!(first_run(&path), Ok(true));
        // The user turns launch at login off (it lives in the OS, not app.toml)
        // and changes a setting; later launches leave it off.
        let prefs = App {
            appearance: "dark".into(),
            ..load_app(&path).unwrap()
        };
        save_app(&path, &prefs).unwrap();
        assert_eq!(first_run(&path), Ok(false));
        assert_eq!(first_run(&path), Ok(false));
        assert_eq!(load_app(&path).unwrap().appearance, "dark");
        // An earlier version made the folder and maybe no app.toml: an
        // upgrade, not a first launch.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(first_run(&path), Ok(false));
        assert!(load_app(&path).unwrap().onboarded);
        // A file with a typo is neither read as a first launch nor overwritten.
        std::fs::write(&path, "appearance = dark\n").unwrap();
        assert!(first_run(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "appearance = dark\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn app_settings_round_trip_and_keep_defaults() {
        let dir =
            std::env::temp_dir().join(format!("everyport-app-settings-{}", std::process::id()));
        let path = dir.join("app.toml");
        assert_eq!(load_app(&path), Ok(App::default()));
        let prefs = App {
            theme: Some("catppuccin".into()),
            appearance: "dark".into(),
            shortcut: "Alt+Space".into(),
            onboarded: true,
        };
        save_app(&path, &prefs).unwrap();
        assert_eq!(load_app(&path), Ok(prefs));
        std::fs::write(&path, "appearance = \"light\"\n").unwrap();
        let partial = load_app(&path).unwrap();
        assert_eq!(partial.appearance, "light");
        assert_eq!(partial.shortcut, "CommandOrControl+Alt+P");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
