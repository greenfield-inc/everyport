//! `ppm remote` and `ppm --on`, run as a user would, with a home folder of
//! their own. The machine `here` is this one, reached through `env`, which
//! passes its arguments on like `docker exec` does.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PPM: &str = env!("CARGO_BIN_EXE_ppm");

struct Home(PathBuf);

impl Home {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ppm-{test}-{}.noindex", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn ppm(&self, args: &[&str]) -> Output {
        // `ppm` on PATH is this build, so `here` has it installed.
        let bin = Path::new(PPM).parent().unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        Command::new(PPM)
            .args(args)
            .env("HOME", &self.0)
            .env("PATH", path)
            .env("PANE_DIR", self.0.join(".pane"))
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn adds_lists_and_removes_machines() {
    let home = Home::new("remote");
    let added = home.ppm(&["remote", "add", "here", "--", "env"]);
    assert!(added.status.success(), "{}", stderr(&added));
    assert_eq!(
        stdout(&added),
        "Added here. Run `ppm --on here` to see its servers.\n"
    );
    // printf '{"url":"http://127.0.0.1:7767","token":"s3cret"}' | base64 | tr '+/' '-_' | tr -d '='
    let code = "ppm://eyJ1cmwiOiJodHRwOi8vMTI3LjAuMC4xOjc3NjciLCJ0b2tlbiI6InMzY3JldCJ9";
    let added = home.ppm(&["remote", "add", "mini", "--code", code]);
    assert!(added.status.success(), "{}", stderr(&added));
    let again = home.ppm(&["remote", "add", "mini", "--code", code]);
    assert!(stdout(&again).starts_with("Updated mini."));

    let list = home.ppm(&["remote", "list"]);
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        stdout(&list),
        format!(
            "NAME  CONNECTION             FROM   PPM\n\
             here  env                    saved  {version}\n\
             mini  http://127.0.0.1:7767  saved  ppm serve\n"
        )
    );
    assert!(!stdout(&list).contains("s3cret"));

    let removed = home.ppm(&["remote", "rm", "mini"]);
    assert_eq!(stdout(&removed), "Removed mini.\n");
    let missing = home.ppm(&["remote", "rm", "mini"]);
    assert!(!missing.status.success());
    assert_eq!(stderr(&missing), "ppm: no saved machine named mini\n");
}

#[test]
fn on_runs_the_command_on_the_named_machine() {
    let home = Home::new("on");
    home.ppm(&["remote", "add", "here", "--", "env"]);

    let list = home.ppm(&["--on", "here", "list", "--json"]);
    assert!(list.status.success(), "{}", stderr(&list));
    let snapshot: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(snapshot["servers"].is_array());

    let unknown = home.ppm(&["--on", "nowhere", "list"]);
    assert!(!unknown.status.success());
    assert_eq!(
        stderr(&unknown),
        "ppm: No machine named nowhere. Add it with `ppm remote add nowhere -- ssh nowhere`, or see `ppm remote list`.\n"
    );

    let local_only = home.ppm(&["--on", "here", "doctor"]);
    assert!(!local_only.status.success());
    assert_eq!(
        stderr(&local_only),
        "ppm: --on works with list, watch, stop, restart, open, clean and the terminal UI\n"
    );
}
