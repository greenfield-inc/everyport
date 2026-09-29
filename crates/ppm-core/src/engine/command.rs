//! Command lines: what to show for a process, and what to run on restart.

use super::tree::stem;

/// Turns raw argv into what someone would have typed, such as
/// `node /…/npm-cli.js run dev` into `npm run dev`.
pub fn pretty(args: &[String], comm: &str) -> String {
    let args = trimmed(args);
    let Some(first) = args.first() else {
        return comm.to_string();
    };
    let executable = without_exe(basename(first));
    let mut parts: Vec<&str> = args.clone();
    if ["node", "bun", "deno"].contains(&stem(executable).as_str()) && args.len() > 1 {
        let script = args[1].replace('\\', "/");
        let tools = [
            "npm",
            "npx",
            "pnpm",
            "yarn",
            "vite",
            "next",
            "astro",
            "nuxt",
            "storybook",
            "tsx",
            "turbo",
        ];
        let tool = tools.into_iter().find(|tool| {
            script.contains(&format!("/{tool}/"))
                || script.contains(&format!("/{tool}-cli"))
                || basename(&script) == *tool
        });
        if let Some(tool) = tool {
            parts.splice(0..2, [tool]);
        } else if is_absolute(&script) || script.starts_with('.') {
            parts.splice(0..2, [basename(args[1])]);
        }
    } else {
        parts[0] = executable;
    }
    let parts: Vec<&str> = parts
        .into_iter()
        .map(|p| if is_absolute(p) { basename(p) } else { p })
        .collect();
    parts.join(" ")
}

/// `pretty`, without the version that titles like `next-server (v16.0.0)` carry.
pub fn display_name(args: &[String], comm: &str) -> String {
    if trimmed(args).is_empty() {
        return comm.to_string();
    }
    let command = pretty(args, comm);
    match command.find(" (") {
        Some(paren) => command[..paren].to_string(),
        None => command,
    }
}

/// Tools like npm and Next.js overwrite their argv with a title, which leaves
/// one argument containing spaces.
pub fn has_intact_args(args: &[String]) -> bool {
    let parts = trimmed(args);
    parts.len() > 1 || (parts.len() == 1 && !parts[0].contains(' '))
}

/// The command line to rerun. A title (`npm run dev -p 3000`) is what was
/// typed, so it runs through a shell as is; intact argv is quoted.
pub fn shell_command(args: &[String]) -> Option<String> {
    let args = trimmed(args);
    let first = *args.first()?;
    let shell = stem_name(first);
    if ["sh", "bash", "zsh", "dash"].contains(&shell.as_str()) && args.len() >= 3 && args[1] == "-c"
    {
        return Some(args[2].to_string());
    }
    if shell == "cmd" {
        if let Some(c) = args.iter().position(|a| a.eq_ignore_ascii_case("/c")) {
            let rest = args[c + 1..].join(" ");
            return Some(rest.trim_matches('"').to_string());
        }
    }
    if args.len() == 1 && first.contains(' ') {
        return Some(first.split(" (").next().unwrap_or(first).to_string());
    }
    Some(args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "))
}

fn trimmed(args: &[String]) -> Vec<&str> {
    args.iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .collect()
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn stem_name(path: &str) -> String {
    stem(basename(path))
}

fn without_exe(name: &str) -> &str {
    let cut = name.len().saturating_sub(4);
    match name.get(cut..) {
        Some(ext) if ext.eq_ignore_ascii_case(".exe") => &name[..cut],
        _ => name,
    }
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

#[cfg(unix)]
fn quote(value: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(windows)]
fn quote(value: &str) -> String {
    if !value.is_empty() && !value.contains([' ', '\t', '"']) {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}
