//! Checks each step of reaching a machine, in order, and stops at the first
//! one that fails, with the fix for it. `everyport doctor` prints the steps,
//! and the desktop app shows them when a machine can't connect.

use crate::client::install::{self, Probe};
use crate::client::machines::Via;
use crate::client::remote;
use crate::client::{Connection, Token, Update};
use crate::protocol::{Event, Os};
use serde::Serialize;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// How long a lookup or a TCP connection may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// ssh never prompts during a check, and gives up on a silent machine.
const BATCH: [&str; 4] = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"];

/// What is known about a machine before reaching it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hint {
    /// Its OS, such as from Tailscale, for the fix when ssh doesn't answer.
    pub os: Option<Os>,
    /// Pane lists it. Pane reaches it over its own connection, not ssh.
    pub pane: bool,
}

/// One step: what was checked, and the fix when it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub label: String,
    pub ok: bool,
    /// One sentence with the fix, for a failed step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    /// What the failing tool printed, behind a Details toggle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub steps: Vec<Step>,
    /// Set once the machine runs commands, even if everyport is missing there.
    pub probe: Option<Probe>,
    /// Trying again won't help until the user acts, such as adding their key.
    pub waits_for_user: bool,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.steps.iter().all(|step| step.ok)
    }
}

/// A line per step, with the fix and what the tool printed under the one
/// that failed.
impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, step) in self.steps.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{} {}", if step.ok { "✓" } else { "✗" }, step.label)?;
            if let Some(fix) = &step.fix {
                write!(f, "\n  {fix}")?;
            }
            for line in step.detail.iter().flat_map(|d| d.lines()) {
                write!(f, "\n    {line}")?;
            }
        }
        Ok(())
    }
}

/// Why ssh couldn't run a command, from what it printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NoName,
    NoAnswer,
    SshOff,
    /// Nothing answers at an `everyport serve` URL.
    ServeOff,
    NewHostKey,
    ChangedHostKey,
    KeyRejected,
}

impl Failure {
    pub fn of(error: &str) -> Option<Self> {
        let has = |text| error.contains(text);
        Some(if has("REMOTE HOST IDENTIFICATION HAS CHANGED") {
            Self::ChangedHostKey
        } else if has("Host key verification failed") {
            Self::NewHostKey
        } else if ssh_denied(error) || has("Too many authentication failures") {
            Self::KeyRejected
        } else if has("Connection refused") {
            Self::SshOff
        } else if has("Could not resolve hostname") {
            Self::NoName
        } else if [
            "timed out",
            "No route to host",
            "Network is unreachable",
            "Host is down",
        ]
        .iter()
        .any(|text| has(text))
        {
            Self::NoAnswer
        } else {
            return None;
        })
    }

    /// True when trying again can't help until the user acts.
    pub fn waits_for_user(self) -> bool {
        matches!(
            self,
            Self::NewHostKey | Self::ChangedHostKey | Self::KeyRejected
        )
    }

    fn label(self, target: &Target) -> String {
        let Target { host, port, .. } = target;
        match self {
            Self::NoName => format!("{host} doesn't resolve"),
            Self::NoAnswer => format!("No answer from {host} on port {port}"),
            Self::SshOff | Self::ServeOff => format!("Nothing listens on port {port}"),
            Self::NewHostKey => "Host key not trusted yet".into(),
            Self::ChangedHostKey => "Host key changed".into(),
            Self::KeyRejected => "Your key isn't accepted".into(),
        }
    }

    /// The sentence and fix for `machine`, whose OS is `os` when known.
    fn fix(self, machine: &str, target: &Target, os: Option<Os>, hint: &Hint) -> String {
        let Target {
            dest, host, port, ..
        } = target;
        // known_hosts names a host on another port `[host]:port`.
        let known_as = if *port == 22 {
            host.clone()
        } else {
            format!("[{host}]:{port}")
        };
        // Windows has no ssh-copy-id, so its PowerShell appends the key itself.
        let copy_key = if cfg!(windows) {
            format!(
                r#"type $env:USERPROFILE\.ssh\id_ed25519.pub | ssh {dest} "cat >> .ssh/authorized_keys""#
            )
        } else {
            format!("ssh-copy-id {dest}")
        };
        let mut fix = match (self, os) {
            (Self::NoName, _) => format!(
                "This computer can't find {host}. Check the name, and that you're on the same network, VPN or tailnet."
            ),
            (Self::NoAnswer, _) => format!(
                "{machine} didn't answer. Check that it's on and awake, and on the same network or tailnet."
            ),
            (Self::SshOff, Some(Os::Macos)) => format!(
                "SSH is off on {machine}. On {machine}, turn on System Settings > General > Sharing > Remote Login."
            ),
            (Self::SshOff, Some(Os::Linux)) => format!(
                "SSH is off on {machine}. On {machine}, run `sudo systemctl enable --now ssh` (sshd on Fedora and Arch)."
            ),
            (Self::SshOff, Some(Os::Windows)) => format!(
                "SSH is off on {machine}. On {machine}, in PowerShell as administrator, run `{WINDOWS_SSHD}`."
            ),
            (Self::SshOff, None) => format!(
                "SSH is off on {machine}. On a Mac, turn on System Settings > General > Sharing > Remote Login. On Linux, run `sudo systemctl enable --now ssh` (sshd on Fedora and Arch). On Windows, in PowerShell as administrator, run `{WINDOWS_SSHD}`."
            ),
            (Self::ServeOff, _) => format!(
                "everyport serve isn't running on {machine}. Start it there with `everyport serve`."
            ),
            (Self::NewHostKey, _) => format!(
                "This computer hasn't seen {machine}'s host key. Run `ssh {dest}` in a terminal once and accept it."
            ),
            (Self::ChangedHostKey, _) => format!(
                "{machine}'s host key changed since you last connected. If you expect that, run `ssh-keygen -R {known_as}`, then `ssh {dest}` to accept the new key."
            ),
            (Self::KeyRejected, Some(Os::Windows)) => format!(
                "{machine} didn't accept your key. Add your public key to {WINDOWS_KEYS} on it. If your key has a passphrase, run `ssh-add` first."
            ),
            (Self::KeyRejected, Some(_)) => format!(
                "{machine} didn't accept your key. Run `{copy_key}` to add it. If your key has a passphrase, run `ssh-add` first."
            ),
            (Self::KeyRejected, None) => format!(
                "{machine} didn't accept your key. Run `{copy_key}` to add it. If your key has a passphrase, run `ssh-add` first. For Windows, add your public key to {WINDOWS_KEYS}."
            ),
        };
        if hint.pane && matches!(self, Self::SshOff | Self::NoAnswer) {
            fix.push_str(&format!(
                " Pane reaches {machine} over its own connection, so SSH may simply be off."
            ));
        }
        fix
    }
}

/// ssh's own `user@host: Permission denied (publickey,password).`, never a
/// file error such as `Permission denied (os error 13)`.
fn ssh_denied(error: &str) -> bool {
    error.split(": Permission denied (").skip(1).any(|rest| {
        [
            "publickey",
            "password",
            "keyboard-interactive",
            "hostbased",
            "gssapi",
        ]
        .iter()
        .any(|method| rest.starts_with(method))
    })
}

const WINDOWS_SSHD: &str = "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Start-Service sshd; Set-Service sshd -StartupType Automatic";
const WINDOWS_KEYS: &str = r"`C:\Users\<you>\.ssh\authorized_keys`, or `C:\ProgramData\ssh\administrators_authorized_keys` if you're an administrator there";

/// Where ssh goes for a prefix, as `ssh -G` resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    /// The ssh arguments after `ssh`, such as `-p 2222 me@devbox`, quoted
    /// for a shell.
    dest: String,
    host: String,
    port: u16,
    /// A ProxyJump or ProxyCommand, which this computer can't reach directly.
    proxied: bool,
}

impl Target {
    async fn of(prefix: &[String]) -> Self {
        let dest = prefix.last().cloned().unwrap_or_default();
        let args: Vec<String> = prefix[1..]
            .iter()
            .map(|arg| {
                if arg.is_empty() || arg.contains(char::is_whitespace) {
                    format!("'{arg}'")
                } else {
                    arg.clone()
                }
            })
            .collect();
        let named_port = prefix
            .windows(2)
            .find(|pair| pair[0] == "-p")
            .and_then(|pair| pair[1].parse().ok());
        let mut target = Self {
            host: dest.rsplit('@').next().unwrap_or(&dest).to_string(),
            dest: args.join(" "),
            port: named_port.unwrap_or(22),
            proxied: false,
        };
        let mut argv = remote::ssh_with(prefix, &["-G"]);
        argv.drain(..1);
        let mut command = tokio::process::Command::new(&prefix[0]);
        command.args(&argv).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        command.creation_flags(remote::CREATE_NO_WINDOW);
        let Ok(Ok(out)) = tokio::time::timeout(TIMEOUT, command.output()).await else {
            return target;
        };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            match line.split_once(' ') {
                Some(("hostname", host)) => target.host = host.to_string(),
                Some(("port", port)) => target.port = port.parse().unwrap_or(target.port),
                Some(("proxyjump" | "proxycommand", value)) if value != "none" => {
                    target.proxied = true;
                }
                _ => {}
            }
        }
        target
    }
}

/// Collects steps until one fails.
#[derive(Default)]
struct Steps(Vec<Step>);

impl Steps {
    fn pass(&mut self, label: String) {
        self.0.push(Step {
            label,
            ok: true,
            fix: None,
            detail: None,
        });
    }

    fn fail(&mut self, label: String, fix: String, detail: impl Into<String>) {
        let detail = detail.into();
        self.0.push(Step {
            label,
            ok: false,
            fix: Some(fix),
            detail: (!detail.is_empty()).then_some(detail),
        });
    }

    fn report(self, probe: Option<Probe>, waits_for_user: bool) -> Report {
        Report {
            steps: self.0,
            probe,
            waits_for_user,
        }
    }
}

/// Checks the way to `machine` step by step. `machine` is the name the user
/// knows it by.
pub async fn check(machine: &str, via: &Via, hint: &Hint) -> Report {
    let mut steps = Steps::default();
    let prefix = match via {
        Via::Url { url, token } => {
            let target = url_target(url);
            if reach(&mut steps, machine, &target, hint).await.is_some() {
                serve_answers(&mut steps, machine, url, token).await;
            }
            return steps.report(None, false);
        }
        Via::Command { command } => command,
    };
    if remote::program(prefix) != "ssh" {
        let through = format!("Runs commands through `{}`", prefix.join(" "));
        return match install::probe(prefix).await {
            Ok(probe) => {
                steps.pass(through);
                remote_os(steps, machine, probe)
            }
            Err(error) => {
                let error = format!("{error:#}");
                let first = error.lines().next().unwrap_or_default().to_string();
                steps.fail(
                    format!("Can't run commands through `{}`", prefix[0]),
                    first,
                    error,
                );
                steps.report(None, false)
            }
        };
    }
    let target = Target::of(prefix).await;
    let mut os = hint.os;
    if !target.proxied {
        match reach(&mut steps, machine, &target, hint).await {
            Some(banner_os) => os = os.or(banner_os),
            None => return steps.report(None, false),
        }
    }
    let batch = remote::ssh_with(prefix, &BATCH);
    if let Err(error) = install::run(&batch, Os::Linux, &["exit"]).await {
        let error = format!("{error:#}");
        let (label, fix, waits) = match Failure::of(&error) {
            Some(failure) => (
                failure.label(&target),
                failure.fix(machine, &target, os, hint),
                failure.waits_for_user(),
            ),
            None => (
                "ssh couldn't sign in".to_string(),
                format!(
                    "ssh couldn't sign in to {machine}. Try `ssh {}` in a terminal to see why.",
                    target.dest
                ),
                false,
            ),
        };
        steps.fail(label, fix, error);
        return steps.report(None, waits);
    }
    steps.pass("ssh signs in with your key".into());
    match install::probe(&batch).await {
        Ok(probe) => remote_os(steps, machine, probe),
        Err(error) => {
            let error = format!("{error:#}");
            let first = error.lines().next().unwrap_or_default().to_string();
            steps.fail("Couldn't tell the machine's OS".into(), first, error);
            steps.report(None, false)
        }
    }
}

/// `everyport serve` accepts the token and says hello, through the same
/// session the app uses.
async fn serve_answers(steps: &mut Steps, machine: &str, url: &str, token: &Token) {
    let connection = Connection::Http {
        url: url.to_string(),
        token: token.clone(),
    };
    let (_client, mut updates) = crate::client::connect(connection);
    let first = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match updates.recv().await {
                Some(Update::Connecting) => continue,
                other => break other,
            }
        }
    })
    .await;
    match first {
        Ok(Some(Update::Event(Event::Hello(hello)))) => steps.pass(format!(
            "everyport serve {} answers and accepts the connection code",
            hello.everyport_version
        )),
        // The session's error already says what to do, such as adding a new code.
        Ok(Some(Update::Disconnected { error, .. })) => {
            steps.fail("everyport serve didn't connect".into(), error, "")
        }
        _ => steps.fail(
            "everyport serve doesn't answer".into(),
            format!(
                "{machine} didn't say hello within 15 s. Check that `everyport serve` runs there."
            ),
            "",
        ),
    }
}

/// The name resolves and the port answers. Returns the OS the ssh banner
/// names, or `None` when a step failed.
async fn reach(
    steps: &mut Steps,
    machine: &str,
    target: &Target,
    hint: &Hint,
) -> Option<Option<Os>> {
    let fail = |steps: &mut Steps, failure: Failure, detail: String| {
        let fix = failure.fix(machine, target, hint.os, hint);
        steps.fail(failure.label(target), fix, detail);
    };
    let lookup = tokio::net::lookup_host((target.host.as_str(), target.port));
    let addresses: Vec<SocketAddr> = match tokio::time::timeout(TIMEOUT, lookup).await {
        Ok(Ok(addresses)) => addresses.collect(),
        Ok(Err(error)) => {
            fail(steps, Failure::NoName, error.to_string());
            return None;
        }
        Err(_) => {
            fail(steps, Failure::NoName, "The lookup timed out.".into());
            return None;
        }
    };
    let Some(first) = addresses.first() else {
        fail(steps, Failure::NoName, String::new());
        return None;
    };
    steps.pass(format!("{} resolves to {}", target.host, first.ip()));
    // Like ssh, try each address, such as ::1 and then 127.0.0.1 for localhost.
    let mut errors = Vec::new();
    let mut refused = false;
    let mut connected = None;
    for address in &addresses {
        match tokio::time::timeout(TIMEOUT, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                connected = Some(stream);
                break;
            }
            Ok(Err(error)) => {
                refused |= error.kind() == std::io::ErrorKind::ConnectionRefused;
                errors.push(format!("{address}: {error}"));
            }
            Err(_) => errors.push(format!("{address}: The connection timed out.")),
        }
    }
    let Some(mut stream) = connected else {
        let failure = match (refused, target.dest.contains("://")) {
            (true, true) => Failure::ServeOff,
            (true, false) => Failure::SshOff,
            (false, _) => Failure::NoAnswer,
        };
        fail(steps, failure, errors.join("\n"));
        return None;
    };
    steps.pass(format!("Port {} answers", target.port));
    // sshd greets first, such as `SSH-2.0-OpenSSH_for_Windows_9.5`.
    let mut banner = [0; 256];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut banner)).await;
    let banner = match read {
        Ok(Ok(n)) => String::from_utf8_lossy(&banner[..n]).into_owned(),
        _ => String::new(),
    };
    Some(banner_os(&banner))
}

fn banner_os(banner: &str) -> Option<Os> {
    if banner.contains("Windows") {
        Some(Os::Windows)
    } else if ["Ubuntu", "Debian", "Raspbian"]
        .iter()
        .any(|distro| banner.contains(distro))
    {
        Some(Os::Linux)
    } else {
        None
    }
}

/// The OS step, then the everyport step.
fn remote_os(mut steps: Steps, machine: &str, probe: Probe) -> Report {
    let os = match probe.os {
        Os::Macos => "macOS",
        Os::Linux => "Linux",
        Os::Windows => "Windows",
    };
    steps.pass(format!("{machine} runs {os} ({})", probe.target));
    match &probe.installed {
        Some(version) if install::older(version, install::VERSION) && probe.everyport_path == probe.install_path => {
            steps.pass(format!("everyport {version} is installed, and connecting updates it to {}", install::VERSION));
        }
        Some(version) => steps.pass(format!("everyport {version} is installed")),
        None => steps.fail(
            "everyport isn't installed".into(),
            format!(
                "Everyport installs itself on {machine}: pick it in the app, or run `everyport --on {machine}`."
            ),
            "",
        ),
    }
    steps.report(Some(probe), false)
}

/// The host and port of `scheme://host:port/path`.
fn url_target(url: &str) -> Target {
    let https = url.starts_with("https://");
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, port.parse().ok()),
        _ => (authority, None),
    };
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    Target {
        dest: url.to_string(),
        host,
        port: port.unwrap_or(if https { 443 } else { 80 }),
        proxied: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from OpenSSH 10.3 with `ssh -o BatchMode=yes`.
    const REFUSED: &str = "ssh: connect to host 127.0.0.1 port 1: Connection refused";
    const TIMED_OUT: &str = "ssh: connect to host 10.255.255.1 port 22: Operation timed out";
    const NO_NAME: &str = "ssh: Could not resolve hostname nosuchhost.invalid: nodename nor servname provided, or not known";
    const NEW_KEY: &str = "Host key verification failed.";
    const CHANGED_KEY: &str = "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n\
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
IT IS POSSIBLE THAT SOMEONE IS DOING SOMETHING NASTY!\n\
Someone could be eavesdropping on you right now (man-in-the-middle attack)!\n\
It is also possible that a host key has just been changed.\n\
The fingerprint for the ED25519 key sent by the remote host is\n\
SHA256:2VKtlK/0zOHUVeFAgXCGD2C+JtBlDZyNPpz8AT0JyZ0.\n\
Please contact your system administrator.\n\
Offending ED25519 key in /tmp/known_hosts:1\n\
Host key for [127.0.0.1]:39022 has changed and you have requested strict checking.\n\
Host key verification failed.";
    const DENIED: &str = "parsas@127.0.0.1: Permission denied (publickey).";
    // From OpenSSH's auth.c, when ssh offers more keys than the server's MaxAuthTries.
    const TOO_MANY_KEYS: &str =
        "Received disconnect from 10.0.0.5 port 22:2: Too many authentication failures";

    #[test]
    fn classifies_what_ssh_prints() {
        // As install::probe wraps it.
        let refused = format!("`uname -sm` failed: {REFUSED}");
        assert_eq!(Failure::of(&refused), Some(Failure::SshOff));
        assert_eq!(Failure::of(TIMED_OUT), Some(Failure::NoAnswer));
        assert_eq!(
            Failure::of("ssh: connect to host 10.0.0.9 port 22: No route to host"),
            Some(Failure::NoAnswer)
        );
        assert_eq!(Failure::of(NO_NAME), Some(Failure::NoName));
        assert_eq!(Failure::of(NEW_KEY), Some(Failure::NewHostKey));
        assert_eq!(Failure::of(CHANGED_KEY), Some(Failure::ChangedHostKey));
        assert_eq!(Failure::of(DENIED), Some(Failure::KeyRejected));
        assert_eq!(Failure::of(TOO_MANY_KEYS), Some(Failure::KeyRejected));
        assert_eq!(Failure::of("bash: everyport: command not found"), None);
        assert_eq!(
            Failure::of("Couldn't run /Applications/Everyport.app/Contents/MacOS/everyport-sidecar: Permission denied (os error 13)"),
            None
        );
        assert_eq!(
            Failure::of("me@box: Permission denied (keyboard-interactive)."),
            Some(Failure::KeyRejected)
        );
    }

    #[test]
    fn only_key_and_host_key_problems_wait_for_the_user() {
        let waiting: Vec<Failure> = [REFUSED, TIMED_OUT, NO_NAME, NEW_KEY, CHANGED_KEY, DENIED]
            .into_iter()
            .filter_map(Failure::of)
            .filter(|f| f.waits_for_user())
            .collect();
        assert_eq!(
            waiting,
            [
                Failure::NewHostKey,
                Failure::ChangedHostKey,
                Failure::KeyRejected
            ]
        );
    }

    fn target() -> Target {
        Target {
            dest: "me@mini".into(),
            host: "mini.tail1234.ts.net".into(),
            port: 22,
            proxied: false,
        }
    }

    #[test]
    fn gives_the_fix_for_the_remote_os() {
        let none = Hint::default();
        assert_eq!(
            Failure::SshOff.fix("Mini", &target(), Some(Os::Macos), &none),
            "SSH is off on Mini. On Mini, turn on System Settings > General > Sharing > Remote Login."
        );
        assert_eq!(
            Failure::SshOff.fix("Mini", &target(), Some(Os::Linux), &none),
            "SSH is off on Mini. On Mini, run `sudo systemctl enable --now ssh` (sshd on Fedora and Arch)."
        );
        assert!(Failure::SshOff
            .fix("Mini", &target(), Some(Os::Windows), &none)
            .contains(
                "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Start-Service sshd"
            ));
        let copy_key = if cfg!(windows) {
            r#"type $env:USERPROFILE\.ssh\id_ed25519.pub | ssh me@mini "cat >> .ssh/authorized_keys""#
        } else {
            "ssh-copy-id me@mini"
        };
        assert_eq!(
            Failure::KeyRejected.fix("Mini", &target(), Some(Os::Linux), &none),
            format!("Mini didn't accept your key. Run `{copy_key}` to add it. If your key has a passphrase, run `ssh-add` first.")
        );
        assert!(Failure::KeyRejected
            .fix("Mini", &target(), Some(Os::Windows), &none)
            .contains(r"C:\ProgramData\ssh\administrators_authorized_keys"));
        assert_eq!(
            Failure::ChangedHostKey.fix("Mini", &target(), None, &none),
            "Mini's host key changed since you last connected. If you expect that, run `ssh-keygen -R mini.tail1234.ts.net`, then `ssh me@mini` to accept the new key."
        );
    }

    #[test]
    fn says_pane_may_reach_a_machine_without_ssh() {
        let pane = Hint {
            os: None,
            pane: true,
        };
        let fix = Failure::SshOff.fix("Mini", &target(), None, &pane);
        assert!(fix.starts_with("SSH is off on Mini. On a Mac, turn on"));
        assert!(
            fix.ends_with("Pane reaches Mini over its own connection, so SSH may simply be off.")
        );
        assert!(!Failure::KeyRejected
            .fix("Mini", &target(), None, &pane)
            .contains("Pane"));
    }

    #[test]
    fn reads_the_os_from_the_ssh_banner() {
        assert_eq!(
            banner_os("SSH-2.0-OpenSSH_for_Windows_9.5\r\n"),
            Some(Os::Windows)
        );
        assert_eq!(
            banner_os("SSH-2.0-OpenSSH_9.6p1 Ubuntu-3ubuntu13.5\r\n"),
            Some(Os::Linux)
        );
        assert_eq!(banner_os("SSH-2.0-OpenSSH_9.8\r\n"), None);
    }

    #[tokio::test]
    async fn stops_at_a_closed_ssh_port_with_the_fix() {
        let via = Via::Command {
            command: ["ssh", "-p", "1", "127.0.0.1"].map(String::from).to_vec(),
        };
        let hint = Hint {
            os: Some(Os::Linux),
            pane: false,
        };
        let report = check("box", &via, &hint).await;
        let summary: Vec<(&str, bool)> = report
            .steps
            .iter()
            .map(|s| (s.label.as_str(), s.ok))
            .collect();
        assert_eq!(
            summary,
            [
                ("127.0.0.1 resolves to 127.0.0.1", true),
                ("Nothing listens on port 1", false)
            ]
        );
        assert_eq!(
            report.steps[1].fix.as_deref(),
            Some("SSH is off on box. On box, run `sudo systemctl enable --now ssh` (sshd on Fedora and Arch).")
        );
        assert!(report.probe.is_none() && !report.waits_for_user && !report.ok());
    }
}
