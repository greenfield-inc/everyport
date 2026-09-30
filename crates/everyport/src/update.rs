//! Checking for a newer Everyport, and the one-line installer that updates to
//! it. The check asks GitHub where `releases/latest` leads and sends nothing else.

use anyhow::Context;
use semver::Version;
use std::time::Duration;

/// Redirects to the newest release's tag, such as `.../releases/tag/v0.1.3`.
const LATEST: &str = "https://github.com/greenfield-inc/everyport/releases/latest";
/// The newest release's files.
const DOWNLOADS: &str = "https://github.com/greenfield-inc/everyport/releases/latest/download";
/// The release page for a version is `{RELEASES}/tag/v{version}`.
pub const RELEASES: &str = "https://github.com/greenfield-inc/everyport/releases";

/// This build's version.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

/// The newest release's version. `EVERYPORT_UPDATE_URL` replaces GitHub's URL,
/// for testing: any URL whose final location ends in the tag, such as
/// `http://127.0.0.1:8000/v0.1.3/`.
pub async fn latest() -> anyhow::Result<Version> {
    let url = std::env::var("EVERYPORT_UPDATE_URL").unwrap_or_else(|_| LATEST.into());
    let response = crate::client::http::builder()
        .timeout(Duration::from_secs(5))
        .build()?
        .head(&url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .with_context(|| format!("couldn't reach {url}"))?;
    let found = response.url();
    tag_version(found.path()).with_context(|| format!("{found} names no release"))
}

/// The version in a tag URL's last segment, with or without its `v`.
fn tag_version(path: &str) -> Option<Version> {
    let tag = path.trim_end_matches('/').rsplit('/').next()?;
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// The version to offer: `latest` when it's newer than `current`, unless the
/// user chose to skip it.
pub fn offer(current: &Version, latest: &Version, skipped: Option<&Version>) -> Option<Version> {
    (latest > current && Some(latest) != skipped).then(|| latest.clone())
}

/// The one-line install command, with the variables it runs with.
#[derive(Clone, Debug, PartialEq)]
pub struct Install {
    pub url: String,
    pub env: Vec<(String, String)>,
}

impl Install {
    /// The desktop app's installer, which also installs the CLI.
    pub fn app() -> Self {
        Self::from_env("https://everyport.dev/install", "install-app")
    }

    /// The CLI's installer.
    pub fn cli() -> Self {
        Self::from_env(&format!("{DOWNLOADS}/install"), "install")
    }

    /// `{default}.sh`, or `.ps1` on Windows. With `EVERYPORT_DOWNLOAD_URL` set,
    /// `{name}.sh` from that folder instead, run with it and
    /// `EVERYPORT_ALLOW_INSECURE`, so a mirror serves the whole update.
    fn from_env(default: &str, name: &str) -> Self {
        let extension = if cfg!(windows) { "ps1" } else { "sh" };
        let url = match std::env::var("EVERYPORT_DOWNLOAD_URL") {
            Ok(base) => format!("{}/{name}.{extension}", base.trim_end_matches('/')),
            Err(_) => format!("{default}.{extension}"),
        };
        let env = ["EVERYPORT_DOWNLOAD_URL", "EVERYPORT_ALLOW_INSECURE"]
            .into_iter()
            .filter_map(|key| Some((key.to_string(), std::env::var(key).ok()?)))
            .collect();
        Self { url, env }
    }

    /// Adds a variable the installer runs with.
    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// The command for `sh`.
    pub fn sh(&self) -> String {
        let env: String = self
            .env
            .iter()
            .map(|(key, value)| format!("{key}={} ", sh_quote(value)))
            .collect();
        format!("curl -fsSL {} | {env}sh", self.url)
    }

    /// The command for PowerShell, Windows PowerShell 5.1 included.
    pub fn powershell(&self) -> String {
        let env: String = self
            .env
            .iter()
            .map(|(key, value)| format!("$env:{key}='{}'; ", value.replace('\'', "''")))
            .collect();
        format!("{env}irm {} | iex", self.url)
    }

    /// The command for this OS's shell: PowerShell on Windows, `sh` elsewhere.
    pub fn line(&self) -> String {
        if cfg!(windows) {
            self.powershell()
        } else {
            self.sh()
        }
    }
}

/// `value` as one `sh` word.
fn sh_quote(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._:-=@+,%".contains(c);
    if !value.is_empty() && value.chars().all(plain) {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn offers_only_a_newer_release_the_user_didnt_skip() {
        assert_eq!(offer(&v("0.1.2"), &v("0.1.3"), None), Some(v("0.1.3")));
        // By number, not text: 0.1.10 comes after 0.1.9.
        assert_eq!(offer(&v("0.1.9"), &v("0.1.10"), None), Some(v("0.1.10")));
        assert_eq!(offer(&v("0.1.3"), &v("0.1.3"), None), None);
        // A prerelease build is ahead of the release before it.
        assert_eq!(offer(&v("0.2.0-beta.1"), &v("0.1.3"), None), None);
        assert_eq!(
            offer(&v("0.2.0-beta.1"), &v("0.2.0"), None),
            Some(v("0.2.0"))
        );

        // Skipping 0.1.3 hides it, and the release after it shows again.
        assert_eq!(offer(&v("0.1.2"), &v("0.1.3"), Some(&v("0.1.3"))), None);
        assert_eq!(
            offer(&v("0.1.2"), &v("0.1.4"), Some(&v("0.1.3"))),
            Some(v("0.1.4"))
        );
    }

    #[test]
    fn reads_the_version_from_the_tag_url() {
        assert_eq!(
            tag_version("/greenfield-inc/everyport/releases/tag/v0.1.3"),
            Some(v("0.1.3"))
        );
        assert_eq!(tag_version("/v0.2.0-beta.1/"), Some(v("0.2.0-beta.1")));
        // No release yet: GitHub leads to the releases page.
        assert_eq!(tag_version("/greenfield-inc/everyport/releases"), None);
    }

    #[test]
    fn install_commands_match_the_documented_one_liners() {
        let app = Install {
            url: "https://everyport.dev/install.sh".into(),
            env: vec![],
        };
        assert_eq!(app.sh(), "curl -fsSL https://everyport.dev/install.sh | sh");
        let app = Install {
            url: "https://everyport.dev/install.ps1".into(),
            env: vec![],
        };
        assert_eq!(
            app.powershell(),
            "irm https://everyport.dev/install.ps1 | iex"
        );
    }

    #[test]
    fn install_variables_survive_spaces_and_quotes() {
        let mac = Install {
            url: "https://everyport.dev/install.sh".into(),
            env: vec![],
        }
        .with("EVERYPORT_APP_DIR", "/Users/me/Bob's Apps")
        .with("EVERYPORT_ALLOW_INSECURE", "1");
        assert_eq!(
            mac.sh(),
            r#"curl -fsSL https://everyport.dev/install.sh | EVERYPORT_APP_DIR='/Users/me/Bob'\''s Apps' EVERYPORT_ALLOW_INSECURE=1 sh"#
        );
        let windows = Install {
            url: "https://everyport.dev/install.ps1".into(),
            env: vec![],
        }
        .with("EVERYPORT_APP_DIR", r"C:\Users\Bob's PC\Everyport");
        assert_eq!(
            windows.powershell(),
            r"$env:EVERYPORT_APP_DIR='C:\Users\Bob''s PC\Everyport'; irm https://everyport.dev/install.ps1 | iex"
        );
    }
}
