//! Machines the user added, stored in `machines.toml` in the ppm config
//! folder. The desktop app and the CLI share the file.
//!
//! ```toml
//! [[machine]]
//! name = "devbox"
//! command = ["ssh", "devbox"]
//!
//! [[machine]]
//! name = "mac-mini"
//! url = "https://mac-mini.tail1234.ts.net"
//! token = "..."
//! ```

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    #[serde(flatten)]
    pub via: Via,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Via {
    /// A command prefix, such as `["ssh", "devbox"]`.
    Command { command: Vec<String> },
    /// A `ppm serve` URL and its token, from a `ppm://` code.
    Url { url: String, token: String },
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "machine")]
    machines: Vec<Machine>,
}

/// `machines.toml` in the ppm config folder (`~/Library/Application Support`,
/// `%APPDATA%` or `~/.config`, under `port-process-manager`).
pub fn path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("port-process-manager")
            .join("machines.toml"),
    )
}

/// The machines in `path`, or none when the file doesn't exist yet.
pub fn load(path: &Path) -> anyhow::Result<Vec<Machine>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("Couldn't read {}", path.display())),
    };
    let file: File =
        toml::from_str(&text).with_context(|| format!("Couldn't read {}", path.display()))?;
    Ok(file.machines)
}

/// Replaces the file with `machines`. Only the user can read it, because it
/// can hold tokens.
pub fn save(path: &Path, machines: &[Machine]) -> anyhow::Result<()> {
    let text = toml::to_string(&File {
        machines: machines.to_vec(),
    })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = path.with_extension("toml.part");
    std::fs::write(&part, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&part, path).with_context(|| format!("Couldn't write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_format_and_saves_it_back() {
        let dir = std::env::temp_dir().join(format!("ppm-machines-{}", std::process::id()));
        let path = dir.join("machines.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"
[[machine]]
name = "devbox"
command = ["ssh", "devbox"]

[[machine]]
name = "mac-mini"
url = "https://mac-mini.tail1234.ts.net"
token = "t0k"
"#,
        )
        .unwrap();
        let expected = vec![
            Machine {
                name: "devbox".into(),
                via: Via::Command {
                    command: vec!["ssh".into(), "devbox".into()],
                },
            },
            Machine {
                name: "mac-mini".into(),
                via: Via::Url {
                    url: "https://mac-mini.tail1234.ts.net".into(),
                    token: "t0k".into(),
                },
            },
        ];
        assert_eq!(load(&path).unwrap(), expected);

        let saved = dir.join("nested").join("machines.toml");
        save(&saved, &expected).unwrap();
        assert_eq!(load(&saved).unwrap(), expected);
        assert_eq!(load(&dir.join("missing.toml")).unwrap(), vec![]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
