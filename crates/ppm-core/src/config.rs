//! The user's scanner settings, stored as `config.toml` in the ppm config
//! folder. The desktop app writes it from Settings, and every `ppm` command
//! reads it, so both honor the same thresholds and protected list.
//!
//! ```toml
//! alert_memory = 2147483648
//! idle_after_secs = 14400
//! protected = ["postgres", "redis-server"]
//! auto_kill = "ask"
//! ```
//!
//! Keys left out take their defaults from `Config::default()`.

use crate::protocol::Config;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// The ppm config folder: `port-process-manager` under `~/Library/Application
/// Support`, `%APPDATA%` or `~/.config`.
pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("port-process-manager"))
}

/// `config.toml` in `dir()`.
pub fn path() -> Option<PathBuf> {
    dir().map(|dir| dir.join("config.toml"))
}

/// The settings in `path`, or the defaults when it doesn't exist yet.
pub fn load(path: &Path) -> io::Result<Config> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e),
    };
    toml::from_str(&text).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), e.message()),
        )
    })
}

/// Replaces the file with `config`.
pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    write(path, &toml::to_string(config).map_err(io::Error::other)?)
}

/// Replaces a settings file so that a reader, or a crash, never leaves half
/// of it: writes a temporary file of its own, flushes it to disk, then
/// renames it over the old one.
pub fn write(path: &Path, text: &str) -> io::Result<()> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = path.with_extension(format!(
        "{}.{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = std::fs::File::create(&part)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&part, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AutoKill;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ppm-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = temp("missing");
        assert_eq!(load(&dir.join("config.toml")).unwrap(), Config::default());
    }

    #[test]
    fn keys_left_out_keep_their_defaults() {
        let dir = temp("partial");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "idle_after_secs = 7200\nprotected = [\"caddy\"]\nauto_kill = \"act\"\n",
        )
        .unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.idle_after_secs, 7200);
        assert_eq!(config.protected, ["caddy"]);
        assert_eq!(config.auto_kill, AutoKill::Act);
        assert_eq!(config.alert_memory, 2048 * 1024 * 1024);
        assert!(!config.vercel_previews);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saves_and_reads_back_every_setting() {
        let dir = temp("round-trip");
        let path = dir.join("nested").join("config.toml");
        let config = Config {
            min_port: 4000,
            max_port: 5000,
            interval_ms: 5000,
            alert_memory: 512 * 1024 * 1024,
            leak_growth: 100 * 1024 * 1024,
            idle_after_secs: 3600,
            long_running_after_secs: 86400,
            protected: vec!["postgres".into(), "caddy".into()],
            auto_kill: AutoKill::Ask,
            vercel_previews: true,
        };
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_file_names_itself() {
        let dir = temp("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "alert_memory = \"lots\"\n").unwrap();
        let error = load(&path).unwrap_err().to_string();
        assert!(error.contains("config.toml"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
