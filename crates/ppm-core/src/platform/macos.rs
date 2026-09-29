//! macOS implementation of [`Platform`], on libproc, sysctl and Mach host
//! statistics. Without root, libproc shows only our own user's sockets, as
//! with `lsof`. Other users' listeners come from `nettop`, which reads every
//! socket through a kernel channel open only to Apple's own tools, whoever
//! starts it. On macOS 27 the `net.inet.tcp.pcblist_n` sysctl behind
//! `netstat` returns no sockets when an app or a non-Apple program started
//! it, so it isn't used.

use super::{
    inbound_connections, unix, Listener, MemoryStats, OtherListener, Platform, ProcDetails,
    ProcInfo, ProcUsage,
};
use crate::protocol::ProcRef;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CStr};
use std::io;
use std::io::Read;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// A scan asks for connections and listeners back to back, so one socket
/// walk younger than this serves both.
const SOCKETS_MAX_AGE: Duration = Duration::from_millis(500);

/// Other users' ports change rarely, and each read starts `nettop`.
const OTHERS_MAX_AGE: Duration = Duration::from_secs(10);

/// `nettop -L 1` exits after one sample, in milliseconds. One that runs
/// longer is killed.
const NETTOP_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Macos {
    host: libc::mach_port_t,
    /// Mach time units to nanoseconds, as numerator and denominator.
    timebase: (u64, u64),
    cpu: unix::CpuSampler,
    sockets: Mutex<Option<(Instant, Sockets)>>,
    others: OnceLock<Arc<Others>>,
}

/// The latest `nettop` read, which a background thread replaces every
/// `OTHERS_MAX_AGE`, or its error.
#[derive(Default)]
struct Others {
    latest: Mutex<Option<Result<Vec<OtherListener>, String>>>,
    ready: Condvar,
}

impl Macos {
    #[allow(deprecated)] // libc points to the `mach2` crate; these calls are stable.
    pub fn new() -> Self {
        let mut timebase = libc::mach_timebase_info { numer: 1, denom: 1 };
        unsafe { libc::mach_timebase_info(&mut timebase) };
        Self {
            host: unsafe { libc::mach_host_self() },
            timebase: (u64::from(timebase.numer), u64::from(timebase.denom.max(1))),
            cpu: unix::CpuSampler::default(),
            sockets: Mutex::new(None),
            others: OnceLock::new(),
        }
    }
}

impl Macos {
    fn sockets(&self) -> io::Result<Sockets> {
        let mut cached = self.sockets.lock().unwrap();
        if let Some((at, sockets)) = cached.as_ref() {
            if at.elapsed() < SOCKETS_MAX_AGE {
                return Ok(sockets.clone());
            }
        }
        let sockets = Sockets::walk()?;
        *cached = Some((Instant::now(), sockets.clone()));
        Ok(sockets)
    }
}

impl Platform for Macos {
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        Ok(self.sockets()?.listeners)
    }

    /// Listeners from `nettop` whose process runs as another user, as last
    /// read in the background. The first call waits for the first read. Root
    /// sees every socket through libproc, so it has none.
    fn other_listeners(&self) -> io::Result<Vec<OtherListener>> {
        let me = unsafe { libc::geteuid() };
        if me == 0 {
            return Ok(Vec::new());
        }
        let others = self.others.get_or_init(|| {
            let others = Arc::new(Others::default());
            let weak = Arc::downgrade(&others);
            thread::spawn(move || refresh_others(&weak, me));
            others
        });
        let latest = others.latest.lock().unwrap();
        let (latest, _) = others
            .ready
            .wait_timeout_while(latest, NETTOP_TIMEOUT, |latest| latest.is_none())
            .unwrap();
        match latest.as_ref() {
            None => Ok(Vec::new()),
            Some(Ok(listeners)) => Ok(listeners.clone()),
            Some(Err(error)) => Err(io::Error::other(error.clone())),
        }
    }

    fn connections(&self) -> Option<HashMap<u16, u32>> {
        self.sockets().ok().map(|s| s.connections)
    }

    fn environment(&self, pid: u32) -> Option<Vec<(String, String)>> {
        Some(unix::env_pairs(procargs(pid)?.1).collect())
    }

    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        Ok(all_pids()?.into_iter().filter_map(proc_info).collect())
    }

    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails> {
        let (args, env) = procargs(pid)?;
        Some(ProcDetails {
            cwd: cwd(pid),
            args,
            env: unix::pick_env(env, env_keys),
        })
    }

    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        let mut info: libc::rusage_info_v4 = unsafe { mem::zeroed() };
        let buffer = (&mut info as *mut libc::rusage_info_v4).cast::<libc::rusage_info_t>();
        if unsafe { libc::proc_pid_rusage(pid as i32, libc::RUSAGE_INFO_V4, buffer) } != 0 {
            return None;
        }
        let ticks = info.ri_user_time + info.ri_system_time;
        Some(ProcUsage {
            memory: info.ri_phys_footprint,
            cpu_time_ns: ticks.saturating_mul(self.timebase.0) / self.timebase.1,
        })
    }

    /// "Memory Used" as Activity Monitor counts it: app memory (anonymous
    /// pages that aren't purgeable) plus wired plus compressed.
    fn memory(&self) -> MemoryStats {
        let total = sysctl_value::<u64>(c"hw.memsize").unwrap_or(0);
        let mut stats: libc::vm_statistics64 = unsafe { mem::zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let status = unsafe {
            libc::host_statistics64(
                self.host,
                libc::HOST_VM_INFO64,
                (&mut stats as *mut libc::vm_statistics64).cast(),
                &mut count,
            )
        };
        if status != libc::KERN_SUCCESS {
            return MemoryStats { total, used: 0 };
        }
        let page = unsafe { libc::vm_page_size } as u64;
        let app = u64::from(stats.internal_page_count).saturating_sub(stats.purgeable_count.into());
        let pages = app + u64::from(stats.wire_count) + u64::from(stats.compressor_page_count);
        MemoryStats {
            total,
            used: (pages * page).min(total),
        }
    }

    fn cpu_percent(&self) -> f32 {
        let mut info: libc::host_cpu_load_info = unsafe { mem::zeroed() };
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        let status = unsafe {
            libc::host_statistics(
                self.host,
                libc::HOST_CPU_LOAD_INFO,
                (&mut info as *mut libc::host_cpu_load_info).cast(),
                &mut count,
            )
        };
        if status != libc::KERN_SUCCESS {
            return 0.0;
        }
        let t = info.cpu_ticks.map(u64::from);
        let busy = t[libc::CPU_STATE_USER as usize]
            + t[libc::CPU_STATE_SYSTEM as usize]
            + t[libc::CPU_STATE_NICE as usize];
        let total = busy + t[libc::CPU_STATE_IDLE as usize];
        self.cpu.percent(busy, total)
    }

    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()> {
        unix::signal(target, force, |pid| {
            proc_info(pid).map(|p| p.proc.started_at)
        })
    }
}

/// TCP sockets of every process we may inspect, read through libproc as
/// `lsof` does.
#[derive(Clone)]
struct Sockets {
    listeners: Vec<Listener>,
    /// Inbound established connections by local port, each socket counted
    /// once.
    connections: HashMap<u16, u32>,
}

impl Sockets {
    fn walk() -> io::Result<Self> {
        let mut listeners = Vec::new();
        let mut listening = Vec::new();
        let mut established = Vec::new();
        let mut counted = HashSet::new();
        for pid in all_pids()? {
            for fd in socket_fds(pid) {
                let Some(tcp) = TcpSocket::read(pid, fd) else {
                    continue;
                };
                match tcp.state {
                    TSI_S_LISTEN => {
                        listening.push((tcp.address, tcp.port));
                        listeners.push(Listener {
                            port: tcp.port,
                            pid,
                            address: tcp.address.to_string(),
                        });
                    }
                    // A socket shared by forked processes is counted once.
                    TSI_S_ESTABLISHED if counted.insert(tcp.id) => {
                        established.push((tcp.address, tcp.port));
                    }
                    _ => {}
                }
            }
        }
        listeners.sort_by_key(|l| (l.port, l.pid));
        listeners.dedup();
        Ok(Self {
            listeners,
            connections: inbound_connections(&listening, established),
        })
    }
}

/// Reads other users' listeners every `OTHERS_MAX_AGE`, failures included,
/// until the platform is dropped.
fn refresh_others(others: &Weak<Others>, me: u32) {
    let mut users = HashMap::new();
    loop {
        let result = nettop()
            .map(|csv| others_in(&csv, me, &mut users))
            .map_err(|error| error.to_string());
        let Some(others) = others.upgrade() else {
            return;
        };
        *others.latest.lock().unwrap() = Some(result);
        others.ready.notify_all();
        drop(others);
        thread::sleep(OTHERS_MAX_AGE);
    }
}

/// The listeners in `nettop` output whose process doesn't run as `me`.
/// `users` caches user names by uid; one with no name shows its number, as
/// `ls -l` does.
fn others_in(csv: &str, me: u32, users: &mut HashMap<u32, String>) -> Vec<OtherListener> {
    nettop_listeners(csv)
        .filter_map(|(pid, port, address)| {
            // Unlike the full info, the short info is readable for every process.
            let info = pidinfo::<libc::proc_bsdshortinfo>(pid, libc::PROC_PIDT_SHORTBSDINFO)?;
            let uid = info.pbsi_uid;
            if uid == me {
                return None;
            }
            let owner = users
                .entry(uid)
                .or_insert_with(|| passwd_name(uid).unwrap_or_else(|| uid.to_string()));
            Some(OtherListener {
                port,
                address,
                owner: Some(owner.clone()),
                pid: Some(pid),
                process_name: Some(c_string(&info.pbsi_comm)),
            })
        })
        .collect()
}

/// The output of `nettop -L 1 -n -m tcp -J state`, killed after
/// `NETTOP_TIMEOUT`.
fn nettop() -> io::Result<String> {
    let mut child = Command::new("/usr/bin/nettop")
        .args(["-L", "1", "-n", "-m", "tcp", "-J", "state"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    // Read while waiting, so a full pipe can't stall nettop.
    let mut stdout = child.stdout.take().expect("piped");
    let reader = thread::spawn(move || {
        let mut csv = String::new();
        stdout.read_to_string(&mut csv).map(|_| csv)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > NETTOP_TIMEOUT {
            let _ = child.kill();
            child.wait()?;
            return Err(io::Error::new(io::ErrorKind::TimedOut, "nettop timed out"));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let csv = reader
        .join()
        .map_err(|_| io::Error::other("nettop reader panicked"))??;
    if !status.success() {
        return Err(io::Error::other(format!("nettop failed: {status}")));
    }
    Ok(csv)
}

/// `(pid, port, address)` for each listener in the output of
/// `nettop -L 1 -n -m tcp -J state`: a `name.pid,,` line per process, then
/// its sockets, such as `tcp4 127.0.0.1:8317<->*:*,Listen,` or
/// `tcp6 *.53<->*.*,Listen,`.
fn nettop_listeners(csv: &str) -> impl Iterator<Item = (u32, u16, String)> + '_ {
    let mut pid = None;
    csv.lines().filter_map(move |line| {
        // The last two columns are the state and an empty one, so a comma in
        // a process name stays in the first column.
        let mut columns = line.rsplitn(3, ',');
        let (Some(""), Some(state), Some(first)) = (columns.next(), columns.next(), columns.next())
        else {
            return None;
        };
        // A process line has no state; a socket line always has one.
        if state.is_empty() {
            pid = first.rsplit_once('.').and_then(|(_, p)| p.parse().ok());
            return None;
        }
        if state != "Listen" {
            return None;
        }
        let (family, sockets) = first.split_once(' ')?;
        let local = sockets.split("<->").next()?;
        let (address, port) = match family {
            "tcp4" => local.rsplit_once(':')?,
            "tcp6" => local.rsplit_once('.')?,
            _ => return None,
        };
        let address = match (address, family) {
            ("*", "tcp4") => "0.0.0.0",
            ("*", _) => "::",
            _ => address,
        };
        Some((pid?, port.parse().ok()?, address.to_string()))
    })
}

fn socket_fds(pid: u32) -> Vec<i32> {
    let size = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDLISTFDS,
            0,
            std::ptr::null_mut(),
            0,
        )
    };
    if size <= 0 {
        return Vec::new();
    }
    let mut fds = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0
        };
        size as usize / mem::size_of::<libc::proc_fdinfo>()
    ];
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            size,
        )
    };
    fds.truncate(read.max(0) as usize / mem::size_of::<libc::proc_fdinfo>());
    fds.into_iter()
        .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
        .map(|fd| fd.proc_fd)
        .collect()
}

const PROC_PIDFDSOCKETINFO: i32 = 3;
const SOCKINFO_TCP: i32 = 2;
const TSI_S_LISTEN: i32 = 1;
const TSI_S_ESTABLISHED: i32 = 4;
const INI_IPV6: u8 = 2;

/// `struct socket_fdinfo` from `<sys/proc_info.h>`, which libc doesn't bind.
/// Offsets are from the macOS SDK header; only the fields read are named.
#[repr(C)]
struct SocketFdInfo {
    /// `proc_fileinfo`, then `socket_info` up to `soi_so`.
    _head: [u8; 160],
    /// `soi_so`, the kernel's id for the socket.
    so: u64,
    _to_kind: [u8; 88],
    /// `soi_kind`.
    kind: i32,
    _rfu: u32,
    /// `soi_proto`: for TCP, a `tcp_sockinfo` that starts with `in_sockinfo`.
    proto: [u8; 528],
}

const _: () = assert!(mem::size_of::<SocketFdInfo>() == 792);

struct TcpSocket {
    id: u64,
    state: i32,
    port: u16,
    address: IpAddr,
}

impl TcpSocket {
    fn read(pid: u32, fd: i32) -> Option<Self> {
        let mut info: SocketFdInfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<SocketFdInfo>() as i32;
        let read = unsafe {
            libc::proc_pidfdinfo(
                pid as i32,
                fd,
                PROC_PIDFDSOCKETINFO,
                (&mut info as *mut SocketFdInfo).cast(),
                size,
            )
        };
        if read != size || info.kind != SOCKINFO_TCP {
            return None;
        }
        // `in_sockinfo`: `insi_lport` at 4 (network order in its first two
        // bytes), `insi_vflag` at 24, `insi_laddr` at 48. `tcpsi_state` at 80.
        let p = &info.proto;
        let local: [u8; 16] = p[48..64].try_into().ok()?;
        Some(Self {
            id: info.so,
            state: i32::from_ne_bytes(p[80..84].try_into().ok()?),
            port: u16::from_be_bytes([p[4], p[5]]),
            address: if p[24] & INI_IPV6 != 0 {
                IpAddr::V6(Ipv6Addr::from(local))
            } else {
                IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(&local[12..]).ok()?))
            },
        })
    }
}

fn all_pids() -> io::Result<Vec<u32>> {
    let estimate = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if estimate <= 0 {
        return Err(io::Error::last_os_error());
    }
    // Room for processes started between the two calls.
    let mut pids = vec![0i32; estimate as usize + 64];
    let bytes = (pids.len() * mem::size_of::<i32>()) as i32;
    let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if count <= 0 {
        return Err(io::Error::last_os_error());
    }
    pids.truncate(count as usize);
    Ok(pids
        .into_iter()
        .filter(|&pid| pid > 0)
        .map(|pid| pid as u32)
        .collect())
}

/// Name, parent and start time. Readable for our own processes, or all with root.
fn proc_info(pid: u32) -> Option<ProcInfo> {
    let info = pidinfo::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO)?;
    let name = c_string(&info.pbi_name);
    Some(ProcInfo {
        proc: ProcRef {
            pid,
            started_at: info.pbi_start_tvsec * 1000 + info.pbi_start_tvusec / 1000,
        },
        parent: (info.pbi_ppid != 0).then_some(info.pbi_ppid),
        name: if name.is_empty() {
            c_string(&info.pbi_comm)
        } else {
            name
        },
    })
}

/// The account name for `uid` from the directory service.
fn passwd_name(uid: u32) -> Option<String> {
    let mut entry: libc::passwd = unsafe { mem::zeroed() };
    let mut buffer = [0 as c_char; 4096];
    let mut result = std::ptr::null_mut();
    unsafe {
        libc::getpwuid_r(
            uid,
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    (!result.is_null()).then(|| {
        unsafe { CStr::from_ptr(entry.pw_name) }
            .to_string_lossy()
            .into_owned()
    })
}

fn pidinfo<T>(pid: u32, flavor: i32) -> Option<T> {
    let mut info: T = unsafe { mem::zeroed() };
    let size = mem::size_of::<T>() as i32;
    let read =
        unsafe { libc::proc_pidinfo(pid as i32, flavor, 0, (&mut info as *mut T).cast(), size) };
    (read == size).then_some(info)
}

fn cwd(pid: u32) -> Option<String> {
    let info = pidinfo::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO)?;
    let path = c_string(info.pvi_cdir.vip_path.as_flattened());
    (!path.is_empty()).then_some(path)
}

/// Arguments and environment from `KERN_PROCARGS2`: `argc`, the executable
/// path padded with NULs to a multiple of 8 bytes, then `argc` arguments and
/// the environment, each NUL-terminated. The padding is computed rather than
/// skipped, so an empty `argv[0]` keeps its place.
fn procargs(pid: u32) -> Option<(Vec<String>, Vec<String>)> {
    let arg_max = usize::try_from(sysctl_value::<i32>(c"kern.argmax")?).ok()?;
    let mut buffer = vec![0u8; arg_max];
    let mut size = arg_max;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let status = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size < mem::size_of::<i32>() {
        return None;
    }
    buffer.truncate(size);
    let (argc, rest) = buffer.split_at(mem::size_of::<i32>());
    let argc = i32::from_ne_bytes(argc.try_into().ok()?).max(0) as usize;
    let exec_len = rest.iter().position(|&b| b == 0)?;
    let mut strings = rest
        .get((exec_len + 1).next_multiple_of(8)..)?
        .split(|&b| b == 0);
    let args = strings
        .by_ref()
        .take(argc)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    let env = strings
        .take_while(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    Some((args, env))
}

/// A fixed-size value from `sysctlbyname`, such as `hw.memsize` (u64).
fn sysctl_value<T: Default>(name: &CStr) -> Option<T> {
    let mut value = T::default();
    let mut size = mem::size_of::<T>();
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut T).cast::<c_void>(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && size == mem::size_of::<T>()).then_some(value)
}

fn c_string(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::nettop_listeners;

    #[test]
    fn nettop_listeners_carry_the_pid_of_the_process_line_above() {
        // From `nettop -L 1 -n -m tcp -J state` on macOS 27.
        let csv = "\
,state,
apsd.1046,,
tcp4 192.168.1.88:62972<->17.57.144.184:5223,Established,
rapportd.1420,,
tcp4 *:49452<->*:*,Listen,
tcp6 *.49452<->*.*,Listen,
Google Chrome H.94042,,
tcp6 fe80::1c2b:3aff:fe4d:5e6f.5060<->*.*,Listen,
cliproxyapi.2118,,
tcp4 127.0.0.1:8317<->*:*,Listen,
tcp4 127.0.0.1:59736<->127.0.0.1:8317,Established,
tcp worker.321,,
tcp4 127.0.0.1:4100<->*:*,Listen,
a,b.654,,
tcp6 ::1.4200<->*.*,Listen,
";
        let found: Vec<_> = nettop_listeners(csv).collect();
        let expected = [
            (1420, 49452, "0.0.0.0"),
            (1420, 49452, "::"),
            (94042, 5060, "fe80::1c2b:3aff:fe4d:5e6f"),
            (2118, 8317, "127.0.0.1"),
            (321, 4100, "127.0.0.1"),
            (654, 4200, "::1"),
        ]
        .map(|(pid, port, address)| (pid, port, address.to_string()));
        assert_eq!(found, expected);
    }
}
