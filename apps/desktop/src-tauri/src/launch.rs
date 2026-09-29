//! Opens things outside the app: URLs, folders, editors, and agent sessions
//! resumed in the user's terminal.

use std::process::Command;

use everyport::protocol::{AgentKind, AgentSession};
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

/// Opens the server in the browser. Another machine's port is forwarded to
/// this one first.
#[tauri::command]
pub async fn open_server_url(app: AppHandle, machine_id: String, port: u16) -> Result<(), String> {
    let url = machines::url(&app, &machine_id, port).await?;
    open_url(&app, &url)
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
/// expect. Without a link, reveals the workspace folder. Only `pane://` links
/// open, since a remote machine's snapshot could carry any URL.
#[tauri::command]
pub fn open_workspace(app: AppHandle, machine_id: String, port: u16) -> Result<(), String> {
    let server = machines::find_server(&app, &machine_id, port)
        .ok_or_else(|| format!("no server on port {port}"))?;
    if let Some(url) = server
        .workspace
        .as_ref()
        .and_then(|w| w.open_url.as_deref())
    {
        if !url.starts_with("pane://") {
            return Err(format!("not a workspace link: {url}"));
        }
        return open_url(&app, url);
    }
    let folder = [&server.project.worktree, &server.project.root, &server.cwd]
        .into_iter()
        .find_map(|path| path.as_deref())
        .ok_or("this server has no folder")?;
    reveal_folder(app, machine_id, folder.to_string())
}

#[tauri::command]
pub fn reveal_folder(app: AppHandle, machine_id: String, path: String) -> Result<(), String> {
    let path = match place(&app, &machine_id)? {
        Place::Local => path,
        Place::Wsl(distro) => wsl_path(&distro, &path),
    };
    reveal(&app, &path)
}

#[tauri::command]
pub fn open_in_editor(app: AppHandle, machine_id: String, path: String) -> Result<(), String> {
    match place(&app, &machine_id)? {
        Place::Local if editor(&path) => Ok(()),
        Place::Local => reveal(&app, &path),
        Place::Wsl(distro) if wsl_editor(&distro, &path) => Ok(()),
        Place::Wsl(distro) => reveal(&app, &wsl_path(&distro, &path)),
    }
}

/// Resumes the session with this id from the latest snapshot. The command is
/// built here from the session's kind and id, never taken from the page or
/// the snapshot, so neither can run anything else in a terminal.
#[tauri::command]
pub fn resume_session(
    app: AppHandle,
    machine_id: String,
    session: AgentSession,
) -> Result<(), String> {
    let place = place(&app, &machine_id)?;
    let (session, cwd) = machines::find_session(&app, &machine_id, &session.id)
        .ok_or("that session is no longer running a server")?;
    let command = resume_command(session.kind, &session.id)
        .ok_or_else(|| format!("unexpected session id: {}", session.id))?;
    let directory = session.directory.or(cwd);
    match place {
        Place::Local => {
            let directory = directory
                .or_else(|| std::env::var("HOME").ok())
                .unwrap_or_else(|| ".".into());
            terminal(&session.id, &directory, &command)
        }
        Place::Wsl(distro) => wsl_terminal(&distro, directory.as_deref().unwrap_or("~"), &command),
    }
    .map_err(|e| e.to_string())
}

/// The command that resumes a session, for ids made of letters, digits, `-`
/// and `_` (Claude Code and Codex use UUIDs).
fn resume_command(kind: AgentKind, id: &str) -> Option<String> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let resume = match kind {
        AgentKind::ClaudeCode => "claude --resume",
        AgentKind::Codex => "codex resume",
    };
    valid.then(|| format!("{resume} {id}"))
}

/// Where a machine's folders and terminals open from this computer.
enum Place {
    Local,
    /// A WSL distro on this Windows computer.
    Wsl(String),
}

fn place(app: &AppHandle, machine_id: &str) -> Result<Place, String> {
    if machine_id == machines::LOCAL {
        return Ok(Place::Local);
    }
    machines::distro(app, machine_id)
        .map(Place::Wsl)
        .ok_or_else(|| "Folders and terminals open only for this computer and WSL.".into())
}

/// A distro's Linux path as Windows reaches it.
fn wsl_path(distro: &str, path: &str) -> String {
    format!(r"\\wsl.localhost\{distro}{}", path.replace('/', r"\"))
}

/// `wsl.exe` running `command` in a login shell, in `directory` in the distro.
fn wsl_command(distro: &str, directory: &str, command: &str) -> Vec<String> {
    [
        "wsl.exe", "-d", distro, "--cd", directory, "--exec", "bash", "-lc", command,
    ]
    .map(String::from)
    .to_vec()
}

/// An editor `open_in_editor` can use: its macOS bundle id, and its command
/// elsewhere.
pub struct Editor {
    pub name: &'static str,
    pub bundle_id: &'static str,
    pub command: &'static str,
}

/// The editors `open_in_editor` tries, in order.
pub const EDITORS: [Editor; 4] = [
    Editor {
        name: "Cursor",
        bundle_id: "com.todesktop.230313mzl4w4u92",
        command: if cfg!(windows) {
            "cursor.cmd"
        } else {
            "cursor"
        },
    },
    Editor {
        name: "VS Code",
        bundle_id: "com.microsoft.VSCode",
        command: if cfg!(windows) { "code.cmd" } else { "code" },
    },
    Editor {
        name: "Zed",
        bundle_id: "dev.zed.Zed",
        command: if cfg!(windows) { "zed.exe" } else { "zed" },
    },
    Editor {
        name: "Sublime Text",
        bundle_id: "com.sublimetext.4",
        command: if cfg!(windows) { "subl.exe" } else { "subl" },
    },
];

/// Opens `path` in the first installed editor.
#[cfg(target_os = "macos")]
fn editor(path: &str) -> bool {
    EDITORS.iter().any(|editor| {
        Command::new("open")
            .args(["-b", editor.bundle_id, path])
            .status()
            .is_ok_and(|s| s.success())
    })
}

#[cfg(not(target_os = "macos"))]
fn editor(path: &str) -> bool {
    EDITORS
        .iter()
        .any(|editor| Command::new(editor.command).arg(path).spawn().is_ok())
}

/// Opens a distro's folder in Cursor or VS Code through their WSL remote.
fn wsl_editor(distro: &str, path: &str) -> bool {
    let remote = format!("wsl+{distro}");
    ["cursor.cmd", "code.cmd"].iter().any(|editor| {
        Command::new(editor)
            .args(["--remote", &remote, path])
            .spawn()
            .is_ok()
    })
}

/// Runs `command` in the distro, in Windows Terminal with the distro's
/// profile, or a new console where it isn't installed.
fn wsl_terminal(distro: &str, directory: &str, command: &str) -> std::io::Result<()> {
    let wsl = wsl_command(distro, directory, command);
    Command::new("wt.exe")
        .args(wt_args(
            ["-p", distro]
                .into_iter()
                .chain(wsl.iter().map(String::as_str)),
        ))
        .spawn()
        .or_else(|_| {
            let mut console = Command::new(&wsl[0]);
            console.args(&wsl[1..]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NEW_CONSOLE: u32 = 0x10;
                console.creation_flags(CREATE_NEW_CONSOLE);
            }
            console.spawn()
        })
        .map(drop)
}

/// Arguments for `wt.exe`, which splits its command line at every `;`, even
/// inside a quoted argument, unless it is escaped as `\;`. A folder from a
/// snapshot must not start a second command. Spaces and quotes need nothing
/// here: `Command` quotes each argument for the Windows command line.
fn wt_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    args.into_iter()
        .map(|arg| arg.replace(';', r"\;"))
        .collect()
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
    let file = std::env::temp_dir().join(format!("everyport-resume-{name}.command"));
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
        .args(wt_args([
            "-d",
            directory,
            "powershell.exe",
            "-NoExit",
            "-Command",
            command,
        ]))
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
        // gnome-terminal takes the command after `--`, xfce4-terminal after
        // `-x`, and the rest after `-e`.
        let flag = match candidate {
            "gnome-terminal" => "--",
            "xfce4-terminal" => "-x",
            _ => "-e",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn quotes_single_quotes_for_the_shell() {
        assert_eq!(sh_quote("/tmp/it's here"), r"'/tmp/it'\''s here'");
    }

    #[test]
    fn resumes_claude_code_and_codex_sessions_by_id() {
        let id = "0b5c7f36-2d8e-4f1a-9c3b-6e2f1a8d4c90";
        assert_eq!(
            resume_command(AgentKind::ClaudeCode, id).as_deref(),
            Some("claude --resume 0b5c7f36-2d8e-4f1a-9c3b-6e2f1a8d4c90")
        );
        assert_eq!(
            resume_command(AgentKind::Codex, id).as_deref(),
            Some("codex resume 0b5c7f36-2d8e-4f1a-9c3b-6e2f1a8d4c90")
        );
    }

    #[test]
    fn reaches_wsl_folders_through_wsl_localhost() {
        assert_eq!(
            wsl_path("Ubuntu", "/home/me/app"),
            r"\\wsl.localhost\Ubuntu\home\me\app"
        );
    }

    #[test]
    fn escapes_semicolons_for_windows_terminal() {
        // Windows Terminal's command line docs: a literal `;` is written `\;`,
        // or it starts a new tab. Folder and command as a snapshot could send them.
        let wsl = wsl_command(
            "Ubuntu",
            "/home/me/my app;calc.exe",
            r#"echo "a;b"; claude --resume 0b5c7f36"#,
        );
        let args = wt_args(
            ["-p", "Ubuntu"]
                .into_iter()
                .chain(wsl.iter().map(String::as_str)),
        );
        assert_eq!(
            args,
            [
                "-p",
                "Ubuntu",
                "wsl.exe",
                "-d",
                "Ubuntu",
                "--cd",
                r"/home/me/my app\;calc.exe",
                "--exec",
                "bash",
                "-lc",
                r#"echo "a\;b"\; claude --resume 0b5c7f36"#,
            ]
        );
    }

    #[test]
    fn refuses_ids_that_carry_shell_syntax() {
        for id in ["", "abc; rm -rf ~", "$(whoami)", "a b", "id\nnext"] {
            assert_eq!(resume_command(AgentKind::ClaudeCode, id), None, "{id}");
        }
    }
}
