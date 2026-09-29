//! Windows implementation of [`Platform`]: IP Helper for listeners, a Toolhelp
//! snapshot for the process table, and a process handle for times, memory,
//! command line, cwd and environment. A process we can't open (elevated or
//! protected) still shows with its port and name.

mod console;

pub use console::run_helper;

use super::{Listener, MemoryStats, Platform, ProcDetails, ProcInfo, ProcUsage};
use crate::protocol::ProcRef;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::mem::{offset_of, MaybeUninit};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;
use windows::core::{Owned, BOOL, PCWSTR};
use windows::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
    PROCESSINFOCLASS,
};
use windows::Win32::Foundation::{
    LocalFree, ERROR_INSUFFICIENT_BUFFER, FILETIME, HANDLE, HLOCAL, HWND, LPARAM, NO_ERROR,
    UNICODE_STRING, WIN32_ERROR, WPARAM,
};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows::Win32::Networking::WinSock::{ADDRESS_FAMILY, AF_INET, AF_INET6};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetProcessTimes, GetSystemTimes, OpenProcess, TerminateProcess, PEB, PROCESS_ACCESS_RIGHTS,
    PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    PROCESS_VM_READ,
};
use windows::Win32::UI::Shell::CommandLineToArgvW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};

// Offsets into the 64-bit RTL_USER_PROCESS_PARAMETERS, which the Windows
// headers leave opaque. They match phnt's ntpebteb.h and are stable since Windows 7.
#[cfg(not(target_pointer_width = "64"))]
compile_error!("the PEB offsets below are for 64-bit Windows only");
const CURRENT_DIRECTORY: usize = 0x38;
const ENVIRONMENT: usize = 0x80;
const ENVIRONMENT_SIZE: usize = 0x3F0;
/// Upper bound on an environment block read from another process.
const MAX_ENVIRONMENT: usize = 1 << 20;

/// FILETIME of 1970-01-01, in 100 ns ticks since 1601.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

pub struct Windows {
    /// Idle and total system time at the previous `cpu_percent` call.
    last_cpu: Mutex<Option<(u64, u64)>>,
}

impl Windows {
    pub fn new() -> Self {
        Self {
            last_cpu: Mutex::new(None),
        }
    }
}

impl Platform for Windows {
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        let v4 = tcp_listeners::<MIB_TCPROW_OWNER_PID>(AF_INET)?
            .into_iter()
            .map(|row| Listener {
                port: port(row.dwLocalPort),
                pid: row.dwOwningPid,
                address: Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).to_string(),
            });
        let v6 = tcp_listeners::<MIB_TCP6ROW_OWNER_PID>(AF_INET6)?
            .into_iter()
            .map(|row| Listener {
                port: port(row.dwLocalPort),
                pid: row.dwOwningPid,
                address: match (Ipv6Addr::from(row.ucLocalAddr), row.dwLocalScopeId) {
                    (ip, 0) => ip.to_string(),
                    (ip, scope) => format!("{ip}%{scope}"),
                },
            });
        Ok(v4.chain(v6).collect())
    }

    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        let snapshot = unsafe { Owned::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?) };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut procs = Vec::new();
        let mut more = unsafe { Process32FirstW(*snapshot, &mut entry) }.is_ok();
        while more {
            let pid = entry.th32ProcessID;
            // Pid 0 is the idle pseudo-process.
            if pid != 0 {
                let started_at = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)
                    .and_then(|p| p.times())
                    .map_or(0, |t| t.started_at);
                procs.push(ProcInfo {
                    proc: ProcRef { pid, started_at },
                    parent: Some(entry.th32ParentProcessID),
                    name: wide(&entry.szExeFile),
                });
            }
            more = unsafe { Process32NextW(*snapshot, &mut entry) }.is_ok();
        }

        // Windows keeps a dead parent's pid in its children, and that pid can
        // be reused. A real parent started no later than its child. A start
        // time of 0 is unknown, so it can't rule a parent out.
        let started: HashMap<u32, u64> = procs
            .iter()
            .map(|p| (p.proc.pid, p.proc.started_at))
            .collect();
        for p in &mut procs {
            p.parent = p.parent.filter(|parent| {
                let child = p.proc.started_at;
                *parent != p.proc.pid
                    && started
                        .get(parent)
                        .is_some_and(|&t| t == 0 || child == 0 || t <= child)
            });
        }
        Ok(procs)
    }

    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails> {
        let process = Process::open_readable(pid)?;
        let parameters = process.parameters();
        Some(ProcDetails {
            cwd: parameters.and_then(|p| process.cwd(p)),
            args: process
                .command_line()
                .map(|c| split_args(&c))
                .unwrap_or_default(),
            env: match parameters {
                Some(p) if !env_keys.is_empty() => process.env(p, env_keys).unwrap_or_default(),
                _ => Vec::new(),
            },
        })
    }

    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        let process = Process::open_readable(pid)?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX2 {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            ..Default::default()
        };
        unsafe {
            GetProcessMemoryInfo(
                *process.0,
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                counters.cb,
            )
        }
        .ok()?;
        Some(ProcUsage {
            // The private working set is Task Manager's Memory column.
            memory: counters.PrivateWorkingSetSize as u64,
            cpu_time_ns: process.times().ok()?.cpu_ns,
        })
    }

    fn memory(&self) -> MemoryStats {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        match unsafe { GlobalMemoryStatusEx(&mut status) } {
            Ok(()) => MemoryStats {
                total: status.ullTotalPhys,
                used: status.ullTotalPhys - status.ullAvailPhys,
            },
            Err(_) => MemoryStats::default(),
        }
    }

    fn cpu_percent(&self) -> f32 {
        let [mut idle, mut kernel, mut user] = [FILETIME::default(); 3];
        let times = unsafe {
            GetSystemTimes(
                Some(&raw mut idle),
                Some(&raw mut kernel),
                Some(&raw mut user),
            )
        };
        if times.is_err() {
            return 0.0;
        }
        // Kernel time includes idle time.
        let now = (ticks(idle), ticks(kernel) + ticks(user));
        let mut last = self.last_cpu.lock().unwrap_or_else(|e| e.into_inner());
        match last.replace(now) {
            Some((idle, total)) if now.1 > total => {
                let total = now.1 - total;
                let busy = total.saturating_sub(now.0.saturating_sub(idle));
                busy as f32 * 100.0 / total as f32
            }
            _ => 0.0,
        }
    }

    /// Graceful stop closes the process's windows. A console process has no
    /// window of its own, so it is terminated, as Node's `process.kill` does,
    /// unless `interrupt` already sent its console a Ctrl+C.
    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()> {
        // Holding the handle keeps the pid from being reused until we're done.
        let process = Process::open(
            target.pid,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
        )?;
        if process.times()?.started_at != target.started_at {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the process exited and its pid was reused",
            ));
        }
        if !force && close_windows(target.pid) {
            return Ok(());
        }
        unsafe { TerminateProcess(*process.0, 1) }?;
        Ok(())
    }

    fn interrupt(&self, tree: &[ProcRef]) -> Vec<ProcRef> {
        console::interrupt(tree)
    }
}

fn port(network_order: u32) -> u16 {
    u16::from_be(network_order as u16)
}

// Both owner-pid tables are a u32 count followed by the rows.
const _: () = assert!(
    offset_of!(MIB_TCPTABLE_OWNER_PID, table) == 4
        && offset_of!(MIB_TCP6TABLE_OWNER_PID, table) == 4
);

/// Rows of the listening TCP table for one address family.
fn tcp_listeners<Row: Copy>(family: ADDRESS_FAMILY) -> io::Result<Vec<Row>> {
    let mut buf: Vec<u32> = Vec::new();
    let mut size = 0u32;
    loop {
        let result = unsafe {
            GetExtendedTcpTable(
                (!buf.is_empty()).then(|| buf.as_mut_ptr().cast()),
                &mut size,
                false,
                family.0.into(),
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        match WIN32_ERROR(result) {
            NO_ERROR => break,
            // The table can grow between the size query and the read.
            ERROR_INSUFFICIENT_BUFFER => buf.resize((size as usize).div_ceil(4), 0),
            error => return Err(io::Error::from_raw_os_error(error.0 as i32)),
        }
    }
    let Some((&count, rows)) = buf.split_first() else {
        return Ok(Vec::new());
    };
    let count = (count as usize).min(size_of_val(rows) / size_of::<Row>());
    Ok(unsafe { std::slice::from_raw_parts(rows.as_ptr().cast::<Row>(), count) }.to_vec())
}

struct Times {
    /// Milliseconds since the Unix epoch.
    started_at: u64,
    cpu_ns: u64,
}

struct Process(Owned<HANDLE>);

impl Process {
    fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> windows::core::Result<Self> {
        unsafe { OpenProcess(access, false, pid).map(|h| Self(Owned::new(h))) }
    }

    /// Opens with memory reads when allowed, else with limited query rights,
    /// which elevated processes still grant.
    fn open_readable(pid: u32) -> Option<Self> {
        Self::open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)
            .or_else(|_| Self::open(pid, PROCESS_QUERY_LIMITED_INFORMATION))
            .ok()
    }

    fn times(&self) -> windows::core::Result<Times> {
        let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
        unsafe { GetProcessTimes(*self.0, &mut created, &mut exited, &mut kernel, &mut user) }?;
        Ok(Times {
            started_at: ticks(created).saturating_sub(UNIX_EPOCH_TICKS) / 10_000,
            cpu_ns: (ticks(kernel) + ticks(user)) * 100,
        })
    }

    fn query<T>(&self, class: PROCESSINFOCLASS, buf: &mut [T]) -> Option<u32> {
        let mut len = 0;
        let status = unsafe {
            NtQueryInformationProcess(
                *self.0,
                class,
                buf.as_mut_ptr().cast(),
                size_of_val(buf) as u32,
                &mut len,
            )
        };
        status.is_ok().then_some(len)
    }

    fn command_line(&self) -> Option<String> {
        let mut len = 0;
        // The first call fails and reports the size it needs.
        let _ = unsafe {
            NtQueryInformationProcess(
                *self.0,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut len,
            )
        };
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(2)];
        self.query(ProcessCommandLineInformation, &mut buf)?;
        // The result is a UNICODE_STRING pointing into `buf`.
        let text = unsafe { &*buf.as_ptr().cast::<UNICODE_STRING>() };
        if text.Buffer.is_null() {
            return None;
        }
        Some(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(text.Buffer.0, text.Length as usize / 2)
        }))
    }

    /// Address of the process's RTL_USER_PROCESS_PARAMETERS.
    fn parameters(&self) -> Option<usize> {
        let mut info = [PROCESS_BASIC_INFORMATION::default()];
        self.query(ProcessBasicInformation, &mut info)?;
        let peb: PEB = self.read(info[0].PebBaseAddress as usize)?;
        Some(peb.ProcessParameters as usize)
    }

    fn cwd(&self, parameters: usize) -> Option<String> {
        let dos_path: UNICODE_STRING = self.read(parameters + CURRENT_DIRECTORY)?;
        let cwd = String::from_utf16_lossy(
            &self.read_wide(dos_path.Buffer.0 as usize, dos_path.Length as usize / 2)?,
        );
        // Windows keeps a trailing backslash on every folder, not only `C:\`.
        Some(match cwd.strip_suffix('\\') {
            Some(dir) if !dir.ends_with(':') => dir.to_string(),
            _ => cwd,
        })
    }

    fn env(&self, parameters: usize, keys: &[&str]) -> Option<Vec<(String, String)>> {
        let block: usize = self.read(parameters + ENVIRONMENT)?;
        let size: usize = self.read(parameters + ENVIRONMENT_SIZE)?;
        let block = self.read_wide(block, size.min(MAX_ENVIRONMENT) / 2)?;
        Some(env_vars(&block, keys))
    }

    fn read<T: Copy>(&self, address: usize) -> Option<T> {
        let mut value = MaybeUninit::<T>::uninit();
        self.read_into(address, value.as_mut_ptr().cast(), size_of::<T>())?;
        Some(unsafe { value.assume_init() })
    }

    fn read_wide(&self, address: usize, len: usize) -> Option<Vec<u16>> {
        let mut buf = vec![0u16; len];
        self.read_into(address, buf.as_mut_ptr().cast(), len * 2)?;
        Some(buf)
    }

    fn read_into(&self, address: usize, buf: *mut c_void, len: usize) -> Option<()> {
        let mut read = 0;
        unsafe {
            ReadProcessMemory(
                *self.0,
                address as *const c_void,
                buf,
                len,
                Some(&raw mut read),
            )
        }
        .ok()?;
        (read == len).then_some(())
    }
}

/// The requested variables from a `KEY=value\0...\0\0` block. Windows
/// variable names ignore case, so `keys` match case-insensitively.
fn env_vars(block: &[u16], keys: &[&str]) -> Vec<(String, String)> {
    block
        .split(|&c| c == 0)
        .map(String::from_utf16_lossy)
        // Per-drive folders are stored as hidden `=C:=C:\dir` entries.
        .filter_map(|entry| {
            let at = entry.get(1..)?.find('=')? + 1;
            let key = keys.iter().find(|k| k.eq_ignore_ascii_case(&entry[..at]))?;
            Some((key.to_string(), entry[at + 1..].to_string()))
        })
        .collect()
}

fn split_args(command_line: &str) -> Vec<String> {
    // CommandLineToArgvW returns our own path for an empty string.
    if command_line.trim().is_empty() {
        return Vec::new();
    }
    let wide: Vec<u16> = command_line.encode_utf16().chain([0]).collect();
    let mut count = 0;
    let argv = unsafe { CommandLineToArgvW(PCWSTR(wide.as_ptr()), &mut count) };
    if argv.is_null() {
        return Vec::new();
    }
    let args = unsafe { std::slice::from_raw_parts(argv, count as usize) }
        .iter()
        .map(|arg| String::from_utf16_lossy(unsafe { arg.as_wide() }))
        .collect();
    unsafe { LocalFree(Some(HLOCAL(argv.cast()))) };
    args
}

/// Posts WM_CLOSE to the process's visible top-level windows. Returns false
/// when it has none.
fn close_windows(pid: u32) -> bool {
    unsafe extern "system" fn visit(hwnd: HWND, state: LPARAM) -> BOOL {
        let (pid, closed) = unsafe { &mut *(state.0 as *mut (u32, bool)) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut owner)) };
        if owner == *pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            *closed |= unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok();
        }
        true.into()
    }
    let mut state = (pid, false);
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut state as isize)) };
    state.1
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}
