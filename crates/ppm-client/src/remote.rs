//! Builds the process that runs a program through a command prefix.
//!
//! Prefixes pass the program's arguments on in one of two ways. `docker exec`,
//! `kubectl exec` and `wsl --exec` hand them to the program as an argv array.
//! `ssh` and `wsl --` join them with spaces and run the result in the remote
//! user's shell, so each argument is quoted for that shell. Arguments made of
//! plain characters mean the same either way, so most commands need no quoting.

use ppm_core::protocol::Os;
use std::process::Stdio;
use tokio::process::Command;

/// The prefix's program name, lowercased, without folder or `.exe`.
pub(crate) fn program(prefix: &[String]) -> String {
    let first = prefix.first().map_or("", String::as_str);
    let name = first.rsplit(['/', '\\']).next().unwrap_or(first);
    let name = name.to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// True when the prefix runs its arguments through the remote shell.
fn joins_into_shell(prefix: &[String]) -> bool {
    match program(prefix).as_str() {
        "ssh" => true,
        "wsl" => !prefix.iter().any(|a| a == "--exec" || a == "-e"),
        _ => false,
    }
}

/// `args` run on the machine behind `prefix` (on this machine when it's empty),
/// with stdin, stdout and stderr piped. `os` is the remote OS, which picks the
/// shell quoting.
pub(crate) fn command(prefix: &[String], os: Os, args: &[&str]) -> Command {
    let mut argv: Vec<String> = prefix.to_vec();
    if joins_into_shell(prefix) {
        argv.extend(args.iter().map(|a| quote(a, os)));
    } else {
        argv.extend(args.iter().map(|a| a.to_string()));
    }
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Keeps a console window from flashing up for each child of the desktop app.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn quote(arg: &str, os: Os) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./:=@%+,".contains(c);
    match os {
        Os::Windows if !arg.is_empty() && arg.chars().all(|c| plain(c) || c == '\\') => arg.into(),
        // cmd.exe, the default shell of OpenSSH on Windows.
        Os::Windows => format!("\"{arg}\""),
        _ if !arg.is_empty() && arg.chars().all(plain) => arg.into(),
        // POSIX single quotes, as Pane's escapeForBash.
        _ => format!("'{}'", arg.replace('\'', r"'\''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(prefix: &[&str], os: Os, args: &[&str]) -> Vec<String> {
        let prefix: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
        let cmd = command(&prefix, os, args);
        let std = cmd.as_std();
        std::iter::once(std.get_program())
            .chain(std.get_args())
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn ssh_quotes_for_the_remote_shell() {
        assert_eq!(
            argv(
                &["ssh", "devbox"],
                Os::Linux,
                &["/home/a b/.local/bin/ppm", "stdio"]
            ),
            ["ssh", "devbox", "'/home/a b/.local/bin/ppm'", "stdio"]
        );
        assert_eq!(
            argv(
                &["/usr/bin/ssh", "devbox"],
                Os::Linux,
                &["/home/o'neil/ppm"]
            ),
            ["/usr/bin/ssh", "devbox", r"'/home/o'\''neil/ppm'"]
        );
        assert_eq!(
            argv(
                &["ssh.exe", "win"],
                Os::Windows,
                &[r"C:\Users\me\AppData\Local\ppm\ppm.exe", "stdio"]
            ),
            [
                "ssh.exe",
                "win",
                r"C:\Users\me\AppData\Local\ppm\ppm.exe",
                "stdio"
            ]
        );
        assert_eq!(
            argv(&["ssh", "win"], Os::Windows, &[r"C:\Users\Jo Doe\ppm.exe"]),
            ["ssh", "win", r#""C:\Users\Jo Doe\ppm.exe""#]
        );
    }

    #[test]
    fn exec_prefixes_pass_argv_unchanged() {
        let path = "/home/a b/.local/bin/ppm";
        for prefix in [
            &["docker", "exec", "-i", "box"][..],
            &["kubectl", "exec", "-i", "pod", "--"],
            &["wsl.exe", "-d", "Ubuntu", "--exec"],
        ] {
            let got = argv(prefix, Os::Linux, &[path, "stdio"]);
            assert_eq!(got[prefix.len()..], [path, "stdio"]);
        }
    }

    #[test]
    fn wsl_without_exec_runs_through_the_distro_shell() {
        assert_eq!(
            argv(
                &["wsl", "-d", "Ubuntu", "--"],
                Os::Linux,
                &["/home/a b/ppm"]
            ),
            ["wsl", "-d", "Ubuntu", "--", "'/home/a b/ppm'"]
        );
    }
}
