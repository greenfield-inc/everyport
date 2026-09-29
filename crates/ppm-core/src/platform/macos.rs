//! macOS implementation of [`Platform`], on libproc, sysctl and Mach host
//! statistics. Without root, other users' processes and sockets are not
//! visible, as with `lsof`.

use super::{unix, Listener, MemoryStats, Platform, ProcDetails, ProcInfo, ProcUsage};
use crate::protocol::ProcRef;
use std::ffi::{c_char, c_void, CStr};
use std::io;
use std::mem;

pub struct Macos {
    host: libc::mach_port_t,
    /// Mach time units to nanoseconds, as numerator and denominator.
    timebase: (u64, u64),
    cpu: unix::CpuSampler,
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
        }
    }
}

impl Platform for Macos {
    /// From `listeners`, which asks libproc for every socket of every process
    /// we may inspect, as `lsof` does.
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        let all = listeners::get_all().map_err(|e| io::Error::other(e.to_string()))?;
        Ok(all
            .into_iter()
            .filter(|l| {
                l.protocol == listeners::Protocol::TCP && l.state == listeners::SocketState::Listen
            })
            .map(|l| Listener {
                port: l.socket.port(),
                pid: l.process.pid,
                address: l.socket.ip().to_string(),
            })
            .collect())
    }

    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        Ok(all_pids()?.into_iter().filter_map(proc_info).collect())
    }

    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails> {
        let (args, env) = procargs(pid)?;
        Some(ProcDetails {
            cwd: cwd(pid),
            args,
            env: unix::pick_env(env.into_iter(), env_keys),
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
