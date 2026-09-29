//! `docs/cli.md` is the `--help` of every `ppm` command. After changing a
//! command, rewrite it with
//! `UPDATE_DOCS=1 cargo test -p port-process-manager --test cli_docs`.

use std::path::PathBuf;
use std::process::Command;

fn help(command: &[String]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ppm"))
        .args(command)
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success(), "ppm {command:?} --help failed");
    String::from_utf8(output.stdout)
        .unwrap()
        .replace("ppm.exe", "ppm")
}

/// The subcommands a help text lists, without `help`.
fn subcommands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(String::from)
        .collect()
}

fn section(command: Vec<String>, doc: &mut String) {
    let text = help(&command);
    let name = ["ppm".to_string()]
        .into_iter()
        .chain(command.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    doc.push_str(&format!("\n## `{name}`\n\n```text\n{text}```\n"));
    for sub in subcommands(&text) {
        let mut path = command.clone();
        path.push(sub);
        section(path, doc);
    }
}

#[test]
fn cli_reference_matches_help() {
    let mut doc = String::from(
        "# CLI reference\n\n\
         Every `ppm` command and its options, exactly as `ppm <command> --help` prints them. \
         For what each one is for, see the [README](../README.md#cli).\n",
    );
    section(Vec::new(), &mut doc);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(&path, &doc).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed.replace("\r\n", "\n") == doc,
        "docs/cli.md is out of date. Run: UPDATE_DOCS=1 cargo test -p port-process-manager --test cli_docs"
    );
}
