//! `everyport list` honors the settings in `config.toml`: the port range, the
//! memory threshold and the protected list.
#![cfg(unix)]

use serde_json::Value;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A server of our own on a free port in 39000-39999.
fn listen() -> TcpListener {
    (39000..40000)
        .find_map(|port| TcpListener::bind(("127.0.0.1", port)).ok())
        .expect("a free port in 39000-39999")
}

/// The everyport config folder under `home`, as `dirs::config_dir` finds it.
fn config_dir(home: &Path) -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home.join(".config")
    };
    base.join("everyport")
}

/// `everyport list --json` with `home` as the home folder.
fn everyport(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_everyport"));
    command
        .args(["list", "--json"])
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME");
    command
}

fn list(home: &Path) -> Value {
    let output = everyport(home).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn list_uses_the_settings_file() {
    let server = listen();
    let port = server.local_addr().unwrap().port();
    let home = std::env::temp_dir().join(format!("everyport-config-home-{}", std::process::id()));
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();

    // Only this test's server is in range; by default it isn't protected.
    let config = dir.join("config.toml");
    std::fs::write(&config, format!("min_port = {port}\nmax_port = {port}\n")).unwrap();
    let snapshot = list(&home);
    let servers = snapshot["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1, "{servers:#?}");
    assert_eq!(servers[0]["port"], port);
    assert_eq!(servers[0]["protected"], false);
    assert_eq!(servers[0]["status"], "running");

    // Protect it by the name everyport shows (Linux cuts names to 15 characters).
    let name = servers[0]["process_name"].as_str().unwrap().to_string();
    std::fs::write(
        &config,
        format!(
            "min_port = {port}\nmax_port = {port}\nalert_memory = 1\nprotected = [\"{name}\"]\n"
        ),
    )
    .unwrap();
    let snapshot = list(&home);
    let servers = snapshot["servers"].as_array().unwrap();
    assert_eq!(servers[0]["protected"], true);
    assert_eq!(servers[0]["status"], "attention");

    std::fs::write(&config, "alert_memory = \"lots\"\n").unwrap();
    let output = everyport(&home).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("config.toml"));

    std::fs::remove_dir_all(&home).unwrap();
    drop(server);
}
