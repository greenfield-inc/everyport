//! Machines the user added, stored in `machines.toml` in the everyport config
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

use crate::connection::Token;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::io::Write;
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
    /// An `everyport serve` URL and its token, from an `everyport://` code.
    Url { url: String, token: Token },
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "machine")]
    machines: Vec<Machine>,
}

/// `machines.toml` in the everyport config folder (`~/Library/Application Support`,
/// `%APPDATA%` or `~/.config`, under `everyport`).
pub fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("everyport").join("machines.toml"))
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
    // A leftover part file would keep its old mode, so start from a new one.
    let _ = std::fs::remove_file(&part);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&part)?.write_all(text.as_bytes())?;
    std::fs::rename(&part, path).with_context(|| format!("Couldn't write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_format_and_saves_it_back() {
        let dir = std::env::temp_dir().join(format!("everyport-machines-{}", std::process::id()));
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
                    token: Token("t0k".into()),
                },
            },
        ];
        assert_eq!(load(&path).unwrap(), expected);

        let saved = dir.join("nested").join("machines.toml");
        save(&saved, &expected).unwrap();
        assert_eq!(load(&saved).unwrap(), expected);
        assert!(!format!("{expected:?}").contains("t0k"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&saved).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(load(&dir.join("missing.toml")).unwrap(), vec![]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
