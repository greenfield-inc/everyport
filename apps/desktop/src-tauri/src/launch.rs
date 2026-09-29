//! Opens things outside the app: URLs, folders, editors, and agent sessions
//! resumed in the user's terminal.

use std::process::Command;

use ppm_client::protocol::AgentSession;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::machines;

fn open_url(app: &AppHandle, url: &str) -> Result<(), String> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

fn reveal(app: &AppHandle, path: &str) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_server_url(app: AppHandle, machine_id: String, port: u16) -> Result<(), String> {
    local_only(&machine_id)?;
    open_url(&app, &format!("http://localhost:{port}"))
}

/// Web links such as a Vercel preview. Only http(s), so the page can't launch
/// arbitrary URL handlers.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("not a web link: {url}"));
    }
    open_url(&app, &url)
}

/// Opens the server's workspace in its app through `workspace.open_url`, such
/// as `pane://open?pane=…&panel=…` for the Pane terminal that started it. That
/// link opens the local app for remote machines too, as Pane remote profiles
/// expect. Without a link, reveals the workspace folder.
#[tauri::command]
pub fn open_workspace(app: AppHandle, machine_id: String, port: u16) -> Result<(), String> {
    let server = machines::find_server(&app, &machine_id, port)
        .ok_or_else(|| format!("no server on port {port}"))?;
    if let Some(url) = server
        .workspace
        .as_ref()
        .and_then(|w| w.open_url.as_deref())
    {
        return open_url(&app, url);
    }
    local_only(&machine_id)?;
    let folder = [&server.project.worktree, &server.project.root, &server.cwd]
        .into_iter()
        .find_map(|path| path.as_deref())
        .ok_or("this server has no folder")?;
    reveal(&app, folder)
}

#[tauri::command]
pub fn reveal_folder(app: AppHandle, machine_id: String, path: String) -> Result<(), String> {
    local_only(&machine_id)?;
    reveal(&app, &path)
}

#[tauri::command]
pub fn open_in_editor(app: AppHandle, machine_id: String, path: String) -> Result<(), String> {
    local_only(&machine_id)?;
    if editor(&path) {
        Ok(())
    } else {
        reveal(&app, &path)
    }
}

#[tauri::command]
pub fn resume_session(machine_id: String, session: AgentSession) -> Result<(), String> {
    local_only(&machine_id)?;
    let directory = session
        .directory
        .clone()
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| ".".into());
    terminal(&session.id, &directory, &session.resume_command).map_err(|e| e.to_string())
}

/// Folders and terminals on other machines need the remote connection, which
/// the app doesn't have yet.
fn local_only(machine_id: &str) -> Result<(), String> {
    if machine_id == machines::LOCAL {
        Ok(())
    } else {
        Err("only available for this computer".into())
    }
}

/// Opens `path` in the first installed editor: Cursor, VS Code, Zed, Sublime.
#[cfg(target_os = "macos")]
fn editor(path: &str) -> bool {
    [
        "com.todesktop.230313mzl4w4u92",
        "com.microsoft.VSCode",
        "dev.zed.Zed",
        "com.sublimetext.4",
    ]
    .iter()
    .any(|id| {
        Command::new("open")
            .args(["-b", id, path])
            .status()
            .is_ok_and(|s| s.success())
    })
}

#[cfg(not(target_os = "macos"))]
fn editor(path: &str) -> bool {
    let editors: &[&str] = if cfg!(windows) {
        &["cursor.cmd", "code.cmd", "zed.exe", "subl.exe"]
    } else {
        &["cursor", "code", "zed", "subl"]
    };
    editors
        .iter()
        .any(|editor| Command::new(editor).arg(path).spawn().is_ok())
}

/// Single-quotes `value` for a POSIX shell.
#[cfg(unix)]
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Runs `command` in `directory` in a new window of the user's terminal: the
/// first one running among Ghostty, iTerm and Warp, or Terminal.
#[cfg(target_os = "macos")]
fn terminal(name: &str, directory: &str, command: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let script = format!("#!/bin/zsh -l\ncd {}\n{command}\n", sh_quote(directory));
    // Terminals run a `.command` file they're asked to open, without prompting.
    let name: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    let file = std::env::temp_dir().join(format!("ppm-resume-{name}.command"));
    std::fs::write(&file, script)?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    let app = [
        "com.mitchellh.ghostty",
        "com.googlecode.iterm2",
        "dev.warp.Warp-Stable",
    ]
    .into_iter()
    .find(|id| is_running(id))
    .unwrap_or("com.apple.Terminal");
    Command::new("open")
        .args(["-b", app])
        .arg(&file)
        .status()
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| std::io::Error::other(format!("could not open {app}")))
        })
}

#[cfg(target_os = "macos")]
fn is_running(bundle_id: &str) -> bool {
    use tauri_nspanel::objc2_app_kit::NSRunningApplication;
    use tauri_nspanel::objc2_foundation::NSString;
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id))
        .is_empty()
}

/// Windows Terminal, or a PowerShell console where it isn't installed.
#[cfg(windows)]
fn terminal(_name: &str, directory: &str, command: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x10;

    Command::new("wt.exe")
        .args([
            "-d",
            directory,
            "powershell.exe",
            "-NoExit",
            "-Command",
            command,
        ])
        .spawn()
        .or_else(|_| {
            Command::new("powershell.exe")
                .args(["-NoExit", "-Command", command])
                .current_dir(directory)
                .creation_flags(CREATE_NEW_CONSOLE)
                .spawn()
        })
        .map(drop)
}

/// The desktop's default terminal: `$TERMINAL`, then Debian's
/// `x-terminal-emulator`, then common ones.
#[cfg(target_os = "linux")]
fn terminal(_name: &str, directory: &str, command: &str) -> std::io::Result<()> {
    let script = format!(
        "cd {} && {command}; exec \"${{SHELL:-sh}}\"",
        sh_quote(directory)
    );
    let preferred = std::env::var("TERMINAL").ok();
    let candidates = preferred.iter().map(String::as_str).chain([
        "x-terminal-emulator",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "xterm",
    ]);
    let mut last = std::io::Error::other("no terminal found");
    for candidate in candidates {
        // gnome-terminal takes the command after `--`; the rest take `-e`.
        let flag = if candidate == "gnome-terminal" {
            "--"
        } else {
            "-e"
        };
        match Command::new(candidate)
            .args([flag, "sh", "-c", &script])
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(last)
}

#[cfg(all(test, unix))]
mod tests {
    use super::sh_quote;

    #[test]
    fn quotes_single_quotes_for_the_shell() {
        assert_eq!(sh_quote("/tmp/it's here"), r"'/tmp/it'\''s here'");
    }
}
