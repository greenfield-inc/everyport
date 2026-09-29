//! Graceful stop for console servers: Ctrl+C, as if typed in the server's
//! terminal.
//!
//! Ctrl+C reaches every process on a console, and sending one means attaching
//! to that console. So ppm starts itself again as a helper with no console of
//! its own. The helper attaches to the server's console and sends Ctrl+C only
//! when every process there is in the server's tree or is a shell that
//! launched it. A shell waiting on a command survives Ctrl+C, just as when the
//! user presses it in that terminal. Anything else there, such as a coding
//! agent or a background job, gets a refusal, and ppm terminates instead.

use super::Windows;
use crate::platform::{Platform, ProcInfo};
use crate::protocol::ProcRef;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    AttachConsole, GenerateConsoleCtrlEvent, GetConsoleProcessList, GetConsoleScreenBufferInfo,
    ReadConsoleOutputCharacterW, SetConsoleCtrlHandler, WriteConsoleInputW,
    CONSOLE_SCREEN_BUFFER_INFO, COORD, CTRL_C_EVENT, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT,
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
};

/// The first argument of a helper process.
const HELPER_ARG: &str = "--ppm-console-ctrl-c";
/// The first word of the helper's reply when it sent Ctrl+C. The processes
/// from the tree that got it follow.
const SENT: &str = "sent";
/// How long the helper stays to answer "Terminate batch job (Y/N)?": the
/// engine's 3 s grace period, and time for a launching shell to ask after
/// the engine kills the rest.
const ANSWER_FOR: Duration = Duration::from_secs(5);
/// Shells that ignore Ctrl+C while they wait on a command.
const SHELLS: &[&str] = &["cmd", "powershell", "pwsh", "bash", "sh"];

/// Sends Ctrl+C to the console of `tree` through a helper. Returns the
/// processes from `tree` that got it.
pub fn interrupt(tree: &[ProcRef]) -> Vec<ProcRef> {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let helper = Command::new(exe)
        .arg(HELPER_ARG)
        .args(tree.iter().map(proc_arg))
        .creation_flags(DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut helper) = helper else {
        return Vec::new();
    };
    let mut line = String::new();
    if let Some(stdout) = helper.stdout.take() {
        let _ = BufReader::new(stdout).read_line(&mut line);
    }
    // After sending, it keeps answering batch prompts, so reap it later.
    std::thread::spawn(move || helper.wait());
    let mut reply = line.split_whitespace();
    if reply.next() != Some(SENT) {
        return Vec::new();
    }
    reply.filter_map(parse_proc).collect()
}

/// Runs the helper and exits when this process was started as one.
pub fn run_helper() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(HELPER_ARG) {
        return;
    }
    let tree: Vec<ProcRef> = args.filter_map(|arg| parse_proc(&arg)).collect();
    // AttachConsole can point the standard handles at the console, so keep
    // the pipe to ppm.
    let mut reply = unsafe { File::from_raw_handle(std::io::stdout().as_raw_handle()) };
    match send(&tree) {
        Ok(Sent { members, shells }) => {
            let members: Vec<String> = members.iter().map(proc_arg).collect();
            let _ = writeln!(reply, "{SENT} {}", members.join(" "));
            drop(reply);
            answer_batch_prompts(&shells);
            std::process::exit(0)
        }
        Err(reason) => {
            let _ = writeln!(reply, "{reason}");
            std::process::exit(1)
        }
    }
}

fn proc_arg(proc: &ProcRef) -> String {
    format!("{}:{}", proc.pid, proc.started_at)
}

fn parse_proc(arg: &str) -> Option<ProcRef> {
    let (pid, started_at) = arg.split_once(':')?;
    Some(ProcRef {
        pid: pid.parse().ok()?,
        started_at: started_at.parse().ok()?,
    })
}

#[derive(Debug, PartialEq)]
struct Sent {
    /// Processes from the tree on the console.
    members: Vec<ProcRef>,
    /// Pids of the shells on the console, from the tree or launching it.
    shells: Vec<(u32, String)>,
}

/// Attaches to the tree's console and sends Ctrl+C when that is safe.
fn send(tree: &[ProcRef]) -> Result<Sent, String> {
    // Ctrl+C reaches the helper too.
    unsafe { SetConsoleCtrlHandler(None, true) }.map_err(|e| e.to_string())?;
    if !tree.iter().any(|t| unsafe { AttachConsole(t.pid) }.is_ok()) {
        return Err("no console".into());
    }
    let procs = Windows::new().processes().map_err(|e| e.to_string())?;
    let attached = console_processes();
    let sent = check(tree, &procs, &attached)?;
    // Ctrl+C goes to whoever is on the console when it is sent. Narrow the
    // time for a process to join after the check.
    if console_processes() != attached {
        return Err("the console changed".into());
    }
    unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) }.map_err(|e| e.to_string())?;
    Ok(sent)
}

/// Ok when every process on the console is in `tree` or is a shell above
/// it, and at least one is in `tree`.
fn check(tree: &[ProcRef], procs: &[ProcInfo], attached: &[u32]) -> Result<Sent, String> {
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.proc.pid, p)).collect();
    let mut launchers = HashSet::new();
    for member in tree {
        let mut pid = member.pid;
        // A parent always started first, but equal start times could loop.
        for _ in 0..procs.len() {
            let Some(parent) = by_pid.get(&pid).and_then(|p| p.parent) else {
                break;
            };
            launchers.insert(parent);
            pid = parent;
        }
    }
    let mut sent = Sent {
        members: Vec::new(),
        shells: Vec::new(),
    };
    for pid in attached {
        let Some(info) = by_pid.get(pid) else {
            return Err(format!("pid {pid} on the console has exited"));
        };
        let name = stem(&info.name);
        let shell = SHELLS.contains(&name.as_str());
        if tree.contains(&info.proc) {
            sent.members.push(info.proc);
        } else if !(shell && launchers.contains(pid)) {
            return Err(format!("{} (pid {pid}) shares the console", info.name));
        }
        if shell {
            sent.shells.push((*pid, name));
        }
    }
    if sent.members.is_empty() {
        return Err("the server isn't on this console".into());
    }
    Ok(sent)
}

/// Pids attached to our console, other than our own.
fn console_processes() -> Vec<u32> {
    let mut pids = vec![0u32; 64];
    loop {
        let count = unsafe { GetConsoleProcessList(&mut pids) } as usize;
        if count <= pids.len() {
            pids.truncate(count);
            break;
        }
        pids.resize(count, 0);
    }
    let own = std::process::id();
    pids.retain(|&pid| pid != own);
    pids
}

/// `cmd.exe` asks "Terminate batch job (Y/N)?" when a batch file such as
/// `npm.cmd` gets Ctrl+C, and waits. Answers yes while `cmd` is on the
/// console, but only once the checked shells are all that is left, so the
/// keys can reach nothing else.
fn answer_batch_prompts(shells: &[(u32, String)]) {
    let is_shell = |pid: &u32| shells.iter().any(|(shell, _)| shell == pid);
    let cmds: Vec<u32> = shells
        .iter()
        .filter(|(_, name)| name == "cmd")
        .map(|(pid, _)| *pid)
        .collect();
    let open = |name| OpenOptions::new().read(true).write(true).open(name);
    let (Ok(input), Ok(output)) = (open("CONIN$"), open("CONOUT$")) else {
        return;
    };
    let deadline = Instant::now() + ANSWER_FOR;
    let mut answered = false;
    while Instant::now() < deadline {
        let attached = console_processes();
        if !attached.iter().any(|pid| cmds.contains(pid)) {
            return;
        }
        match prompt_answer(&cursor_line(&output)) {
            // Answer once, then wait for the prompt to go before the next.
            Some(yes) if !answered && attached.iter().all(is_shell) => {
                answered = type_line(&input, yes);
            }
            Some(_) => {}
            None => answered = false,
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The text before the cursor on its line.
fn cursor_line(output: &File) -> String {
    let handle = HANDLE(output.as_raw_handle());
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) }.is_err() {
        return String::new();
    }
    let cursor = info.dwCursorPosition;
    let mut text = vec![0u16; cursor.X.max(0) as usize];
    let mut read = 0;
    let start = COORD { X: 0, Y: cursor.Y };
    if unsafe { ReadConsoleOutputCharacterW(handle, &mut text, start, &mut read) }.is_err() {
        return String::new();
    }
    String::from_utf16_lossy(&text[..read as usize])
}

/// The yes answer when `line` ends in a choice such as `(Y/N)?`. Localized
/// `cmd.exe` asks with its own letters, as in German `(J/N)?`.
fn prompt_answer(line: &str) -> Option<char> {
    let choices = line.trim_end().strip_suffix(")?")?.rsplit_once('(')?.1;
    match choices.chars().collect::<Vec<_>>()[..] {
        [yes, '/', no] if yes.is_alphabetic() && no.is_alphabetic() => Some(yes),
        _ => None,
    }
}

/// Types `key` and Enter into the console.
fn type_line(input: &File, key: char) -> bool {
    const VK_RETURN: u16 = 0x0D;
    let press = |vk: u16, ch: u16, down: bool| INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: down.into(),
                wRepeatCount: 1,
                wVirtualKeyCode: vk,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: ch },
                ..Default::default()
            },
        },
    };
    let ch = key as u16;
    let vk = key.to_ascii_uppercase() as u16;
    let events = [
        press(vk, ch, true),
        press(vk, ch, false),
        press(VK_RETURN, '\r' as u16, true),
        press(VK_RETURN, '\r' as u16, false),
    ];
    let mut written = 0;
    let handle = HANDLE(input.as_raw_handle());
    unsafe { WriteConsoleInputW(handle, &events, &mut written) }.is_ok()
}

fn stem(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    match name.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, name: &str) -> ProcInfo {
        ProcInfo {
            proc: ProcRef {
                pid,
                started_at: u64::from(pid),
            },
            parent: Some(parent),
            name: name.into(),
        }
    }

    fn at(pid: u32) -> ProcRef {
        ProcRef {
            pid,
            started_at: u64::from(pid),
        }
    }

    #[test]
    fn a_console_shared_only_with_launching_shells_gets_ctrl_c() {
        // Windows Terminal: pwsh > npm.cmd in cmd > node npm-cli > cmd /c vite > node.
        let procs = [
            proc(10, 1, "pwsh.exe"),
            proc(20, 10, "cmd.exe"),
            proc(30, 20, "node.exe"),
            proc(40, 30, "cmd.exe"),
            proc(50, 40, "node.exe"),
        ];
        let tree = [at(50), at(40), at(30)];
        let shells = [(10, "pwsh"), (20, "cmd"), (40, "cmd")];
        assert_eq!(
            check(&tree, &procs, &[10, 20, 30, 40, 50]),
            Ok(Sent {
                members: vec![at(30), at(40), at(50)],
                shells: shells.map(|(pid, name)| (pid, name.to_string())).to_vec(),
            })
        );
    }

    #[test]
    fn an_agent_or_a_background_job_on_the_console_is_refused() {
        // A coding agent ran `bash -c "npm run dev"`.
        let agent = [
            proc(10, 1, "claude.exe"),
            proc(20, 10, "bash.exe"),
            proc(30, 20, "node.exe"),
        ];
        assert_eq!(
            check(&[at(30)], &agent, &[10, 20, 30]),
            Err("claude.exe (pid 10) shares the console".into())
        );

        // `start /b ping ... & node server.js` in one cmd.
        let job = [
            proc(10, 1, "cmd.exe"),
            proc(20, 10, "PING.EXE"),
            proc(30, 10, "node.exe"),
        ];
        assert_eq!(
            check(&[at(30)], &job, &[10, 20, 30]),
            Err("PING.EXE (pid 20) shares the console".into())
        );
    }

    #[test]
    fn a_reused_pid_or_a_console_without_the_server_is_refused() {
        let procs = [proc(10, 1, "cmd.exe"), proc(30, 10, "node.exe")];
        let reused = ProcRef {
            pid: 30,
            started_at: 5,
        };
        assert!(check(&[reused], &procs, &[10, 30]).is_err());
        assert!(check(&[at(30)], &procs, &[10]).is_err());
    }

    #[test]
    fn batch_prompts_are_answered_with_their_yes_letter() {
        assert_eq!(prompt_answer("Terminate batch job (Y/N)? "), Some('Y'));
        assert_eq!(prompt_answer("Batchvorgang abbrechen (J/N)? "), Some('J'));
        assert_eq!(prompt_answer("C:\\dev\\app>"), None);
        assert_eq!(prompt_answer("  VITE ready in 300 ms (took 1/2)?"), None);
        assert_eq!(prompt_answer(""), None);
    }
}
