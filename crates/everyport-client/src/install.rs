//! Finds out what a machine runs, and installs the matching `everyport` on it
//! through the same command prefix. The caller asks the user in between.
//!
//! Unix machines get `~/.local/bin/everyport`, Windows machines
//! `%LOCALAPPDATA%\everyport\everyport.exe`. Every step runs a plain command through the
//! prefix, so a machine needs no shell tricks beyond what `ssh` or `docker
//! exec` already give.

use crate::remote;
use anyhow::{anyhow, bail, Context};
use base64::Engine as _;
use everyport_core::protocol::Os;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// The `everyport` version this app installs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const RELEASES: &str = "https://github.com/greenfield-inc/everyport/releases/download";
/// How long a command may take, plus time for its input at 32 KB/s (a
/// 256 kbit/s uplink), so copying the binary never times out on a slow link.
const TIMEOUT: Duration = Duration::from_secs(30);
const SLOWEST_UPLOAD: u64 = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub os: Os,
    /// Rust target triple of the release binary for this machine, such as
    /// `x86_64-unknown-linux-musl`.
    pub target: String,
    /// Where `install` puts `everyport`.
    pub install_path: String,
    /// The `everyport` to run: `install_path`, or `everyport` when only the machine's
    /// `PATH` has it, as after `brew install`.
    pub everyport_path: String,
    /// `everyport --version` there, such as `0.1.0`. `None` when it isn't installed.
    pub installed: Option<String>,
}

impl Probe {
    /// True when the installed `everyport` matches this app.
    pub fn up_to_date(&self) -> bool {
        self.installed.as_deref() == Some(VERSION)
    }
}

/// Checks the machine's OS and CPU, and the `everyport` installed there. Looks at
/// the install path first, then `everyport` on the machine's `PATH`.
pub async fn probe(prefix: &[String]) -> anyhow::Result<Probe> {
    let (os, target, install_path) = match run(prefix, Os::Linux, &["uname", "-sm"]).await {
        Ok(uname) if !is_windows_shell(&uname) => {
            let (os, target) = unix_target(&uname)?;
            let home = run(prefix, os, &["printenv", "HOME"]).await?;
            (
                os,
                target,
                format!("{}/.local/bin/everyport", home.trim_end_matches('/')),
            )
        }
        uname => {
            // Git Bash and MSYS answer `uname` too, but everyport needs the Windows build.
            let script = "Write-Output $env:PROCESSOR_ARCHITECTURE; Write-Output $env:LOCALAPPDATA";
            let out = powershell(prefix, script, &[])
                .await
                .map_err(|windows| match uname {
                    Err(unix) => anyhow!("{unix}"),
                    Ok(_) => windows,
                })?;
            let mut lines = out.lines().map(str::trim);
            let (Some(arch), Some(local)) = (lines.next(), lines.next()) else {
                bail!("Couldn't read the machine's CPU and app data folder.");
            };
            let target = match arch.to_ascii_uppercase().as_str() {
                "AMD64" => "x86_64-pc-windows-msvc",
                "ARM64" => "aarch64-pc-windows-msvc",
                other => bail!("everyport has no build for Windows on {other}."),
            };
            (
                Os::Windows,
                target.to_string(),
                format!(r"{local}\everyport\everyport.exe"),
            )
        }
    };
    let (everyport_path, installed) = match version(prefix, os, &install_path).await {
        Some(v) => (install_path.clone(), Some(v)),
        None => match version(prefix, os, "everyport").await {
            Some(v) => ("everyport".to_string(), Some(v)),
            None => (install_path.clone(), None),
        },
    };
    Ok(Probe {
        os,
        target,
        install_path,
        everyport_path,
        installed,
    })
}

/// Downloads the `everyport` build for `probe.target`, checks its SHA-256, copies it
/// to the machine through the prefix, and checks it runs. `EVERYPORT_BINARY_DIR`
/// points at a folder of local builds named like the release files
/// (`everyport-<target>`, `everyport-<target>.exe`), for development.
pub async fn install(prefix: &[String], probe: &Probe) -> anyhow::Result<()> {
    let binary = binary(&probe.target).await?;
    let sha = hex(&Sha256::digest(&binary));
    let path = &probe.install_path;
    match probe.os {
        Os::Windows => install_windows(prefix, path, &binary, &sha).await?,
        os => {
            let (dir, _) = path
                .rsplit_once('/')
                .context("The install path has no folder.")?;
            run(prefix, os, &["mkdir", "-p", dir]).await?;
            let part = format!("{path}.part");
            send(
                prefix,
                os,
                &["dd", &format!("of={part}"), "bs=65536"],
                &binary,
            )
            .await?;
            let copied = match run(prefix, os, &["sha256sum", &part]).await {
                Ok(out) => Some(out),
                Err(_) => run(prefix, os, &["shasum", "-a", "256", &part]).await.ok(),
            };
            // A machine with neither tool still gets the version check below.
            if copied.is_some_and(|out| !out.starts_with(&sha)) {
                let _ = run(prefix, os, &["rm", "-f", &part]).await;
                bail!("The copy on the machine doesn't match the download. Try again.");
            }
            run(prefix, os, &["chmod", "755", &part]).await?;
            run(prefix, os, &["mv", "-f", &part, path]).await?;
        }
    }
    match version(prefix, probe.os, path).await {
        Some(v) if v == VERSION => Ok(()),
        Some(v) => {
            bail!("Installed everyport {VERSION}, but the machine runs everyport {v} from {path}.")
        }
        None => bail!("Installed everyport to {path}, but it doesn't run there."),
    }
}

async fn install_windows(
    prefix: &[String],
    path: &str,
    binary: &[u8],
    sha: &str,
) -> anyhow::Result<()> {
    let path = path.replace('\'', "''");
    // A running everyport.exe can be renamed but not replaced.
    let script = format!(
        "$ErrorActionPreference = 'Stop'
$exe = '{path}'
New-Item -ItemType Directory -Force -Path (Split-Path $exe) | Out-Null
$part = \"$exe.part\"
$in = [Console]::OpenStandardInput()
$out = [IO.File]::Create($part)
$in.CopyTo($out)
$out.Close()
if ((Get-FileHash -Algorithm SHA256 $part).Hash -ne '{sha}') {{
  Remove-Item $part
  throw 'The copy on the machine does not match the download. Try again.'
}}
if (Test-Path $exe) {{
  Remove-Item \"$exe.old\" -ErrorAction SilentlyContinue
  Move-Item -Force $exe \"$exe.old\"
}}
Move-Item $part $exe"
    );
    powershell(prefix, &script, binary).await?;
    Ok(())
}

/// The release binary for `target`, checked against `SHA256SUMS`.
async fn binary(target: &str) -> anyhow::Result<Vec<u8>> {
    let name = if target.contains("windows") {
        format!("everyport-{target}.exe")
    } else {
        format!("everyport-{target}")
    };
    if let Some(dir) = std::env::var_os("EVERYPORT_BINARY_DIR").map(PathBuf::from) {
        let binary = std::fs::read(dir.join(&name))
            .with_context(|| format!("EVERYPORT_BINARY_DIR has no {name}"))?;
        if let Ok(sums) = std::fs::read_to_string(dir.join("SHA256SUMS")) {
            verify(&binary, &name, &sums)?;
        }
        return Ok(binary);
    }
    let base = format!("{RELEASES}/v{VERSION}");
    let http = crate::http::builder().build()?;
    let get = |file: String| {
        let url = format!("{base}/{file}");
        let http = http.clone();
        async move {
            let response = http
                .get(url)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .with_context(|| format!("Couldn't download {file} for everyport {VERSION}"))?;
            anyhow::Ok(response.bytes().await?)
        }
    };
    let sums = get("SHA256SUMS".into()).await?;
    let binary = get(name.clone()).await?.to_vec();
    verify(&binary, &name, &String::from_utf8_lossy(&sums))?;
    Ok(binary)
}

/// Checks `binary` against its line in a `sha256sum` style file.
fn verify(binary: &[u8], name: &str, sums: &str) -> anyhow::Result<()> {
    let expected = sums
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, file)| file.trim_start().trim_start_matches('*') == name)
        .map(|(sha, _)| sha.to_ascii_lowercase())
        .with_context(|| format!("SHA256SUMS has no line for {name}"))?;
    if hex(&Sha256::digest(binary)) != expected {
        bail!("{name} doesn't match its SHA-256 checksum.");
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_windows_shell(uname: &str) -> bool {
    ["MINGW", "MSYS", "CYGWIN"]
        .iter()
        .any(|p| uname.starts_with(p))
}

/// Maps `uname -sm` output, such as `Linux x86_64`, to a release target.
fn unix_target(uname: &str) -> anyhow::Result<(Os, String)> {
    let mut words = uname.split_whitespace();
    let (os, arch) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => bail!("everyport has no build for {uname}."),
    };
    match os {
        "Linux" => Ok((Os::Linux, format!("{arch}-unknown-linux-musl"))),
        "Darwin" => Ok((Os::Macos, format!("{arch}-apple-darwin"))),
        _ => bail!("everyport has no build for {uname}."),
    }
}

/// `everyport --version` at `path`, as `0.1.0`.
async fn version(prefix: &[String], os: Os, path: &str) -> Option<String> {
    let out = run(prefix, os, &[path, "--version"]).await.ok()?;
    let line = out.lines().next()?;
    line.starts_with("everyport ")
        .then(|| line.split_whitespace().last())
        .flatten()
        .map(String::from)
}

/// Runs a PowerShell script through the prefix, with `input` on stdin.
async fn powershell(prefix: &[String], script: &str, input: &[u8]) -> anyhow::Result<String> {
    // -EncodedCommand takes base64 of UTF-16LE, which needs no quoting in any shell.
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let args = [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &encoded,
    ];
    send(prefix, Os::Windows, &args, input).await
}

/// Runs `args` through the prefix and returns its trimmed stdout.
async fn run(prefix: &[String], os: Os, args: &[&str]) -> anyhow::Result<String> {
    send(prefix, os, args, &[]).await
}

/// Runs `args` through the prefix with `input` on stdin.
async fn send(prefix: &[String], os: Os, args: &[&str], input: &[u8]) -> anyhow::Result<String> {
    let mut command = remote::command(prefix, os, args);
    let program = command
        .as_std()
        .get_program()
        .to_string_lossy()
        .into_owned();
    let mut child = command
        .spawn()
        .with_context(|| format!("Couldn't run {program}"))?;
    let mut stdin = child.stdin.take().context("no stdin")?;
    let timeout = TIMEOUT + Duration::from_secs(input.len() as u64 / SLOWEST_UPLOAD);
    let input = input.to_vec();
    let writer = tokio::spawn(async move {
        let result = stdin.write_all(&input).await;
        drop(stdin);
        result
    });
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .with_context(|| format!("{program} didn't answer within {} s", timeout.as_secs()))??;
    let written = writer.await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let reason = stderr.trim();
        bail!(
            "`{}` failed: {}",
            args.join(" "),
            if reason.is_empty() {
                out.status.to_string()
            } else {
                reason.to_string()
            }
        );
    }
    written.context("The machine stopped reading the file")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_uname_to_release_targets() {
        let target = |uname| unix_target(uname).map(|(_, t)| t).ok();
        assert_eq!(
            target("Linux x86_64").as_deref(),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(
            target("Linux aarch64").as_deref(),
            Some("aarch64-unknown-linux-musl")
        );
        assert_eq!(
            target("Darwin arm64").as_deref(),
            Some("aarch64-apple-darwin")
        );
        assert_eq!(
            target("Darwin x86_64").as_deref(),
            Some("x86_64-apple-darwin")
        );
        assert_eq!(target("FreeBSD amd64"), None);
        assert_eq!(target("Linux armv7l"), None);
        assert!(is_windows_shell("MINGW64_NT-10.0-22631 x86_64"));
    }

    #[test]
    fn checks_the_sha256sums_line() {
        // `printf 'hello\n' | sha256sum`
        let sums = "0000000000000000000000000000000000000000000000000000000000000000  install.sh\n\
                    5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03  everyport-x86_64-unknown-linux-musl\n";
        assert!(verify(b"hello\n", "everyport-x86_64-unknown-linux-musl", sums).is_ok());
        assert!(verify(b"hello!\n", "everyport-x86_64-unknown-linux-musl", sums).is_err());
        assert!(verify(b"hello\n", "everyport-aarch64-apple-darwin", sums).is_err());
    }
}
