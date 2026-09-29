//! Linux implementation of [`Platform`], on `/proc`. Without root, other
//! users' processes show name, parent, start time and resident memory only,
//! and their sockets are not visible.

use super::{unix, Listener, MemoryStats, Platform, ProcDetails, ProcInfo, ProcUsage};
use crate::protocol::ProcRef;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;

pub struct Linux {
    /// Clock ticks per second, the unit of times in `/proc/<pid>/stat`.
    hz: u64,
    /// Boot time in ms since the epoch. Read once so start times stay stable
    /// when the wall clock is adjusted.
    boot_ms: u64,
    /// Our effective uid. Only root can see which process holds another
    /// user's socket.
    uid: u32,
    /// Processes that held listening sockets at the previous scan. They are
    /// searched first, so a steady scan reads only their descriptors.
    socket_owners: Mutex<Vec<u32>>,
    cpu: unix::CpuSampler,
}

impl Linux {
    pub fn new() -> Self {
        let btime = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|stat| stat_line(&stat, "btime").and_then(|v| v.trim().parse::<u64>().ok()));
        Self {
            hz: u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
                .ok()
                .filter(|&hz| hz > 0)
                .unwrap_or(100),
            boot_ms: btime.unwrap_or(0) * 1000,
            uid: unsafe { libc::geteuid() },
            socket_owners: Mutex::new(Vec::new()),
            cpu: unix::CpuSampler::default(),
        }
    }

    fn proc_info(&self, pid: u32) -> Option<ProcInfo> {
        let stat = Stat::read(pid)?;
        Some(ProcInfo {
            proc: ProcRef {
                pid,
                started_at: self.boot_ms + stat.start_ticks * 1000 / self.hz,
            },
            parent: (stat.ppid != 0).then_some(stat.ppid),
            name: stat.name,
        })
    }
}

impl Platform for Linux {
    /// LISTEN rows of `/proc/net/tcp{,6}`, mapped to a process by finding the
    /// socket's inode among the links in `/proc/<pid>/fd`. The search stops
    /// once every socket has an owner.
    fn listeners(&self) -> io::Result<Vec<Listener>> {
        let mut unowned: HashMap<u64, (u16, IpAddr)> = HashMap::new();
        for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
            let Ok(table) = fs::read_to_string(table) else {
                continue;
            };
            for row in table.lines().skip(1).filter_map(ListenRow::parse) {
                if self.uid == 0 || row.uid == self.uid {
                    unowned.insert(row.inode, (row.port, row.address));
                }
            }
        }
        let mut owners = self.socket_owners.lock().unwrap();
        let mut found = Vec::new();
        for pid in owners.iter().copied().chain(pids()?) {
            if unowned.is_empty() {
                break;
            }
            let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Some(inode) = fs::read_link(fd.path()).ok().and_then(|l| socket_inode(&l))
                else {
                    continue;
                };
                if let Some((port, address)) = unowned.remove(&inode) {
                    found.push(Listener {
                        port,
                        pid,
                        address: address.to_string(),
                    });
                }
            }
        }
        *owners = found.iter().map(|l| l.pid).collect();
        owners.dedup();
        Ok(found)
    }

    fn processes(&self) -> io::Result<Vec<ProcInfo>> {
        Ok(pids()?.filter_map(|pid| self.proc_info(pid)).collect())
    }

    fn details(&self, pid: u32, env_keys: &[&str]) -> Option<ProcDetails> {
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let environ = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        Some(ProcDetails {
            cwd: fs::read_link(format!("/proc/{pid}/cwd"))
                .ok()
                .map(|path| path.to_string_lossy().into_owned()),
            args: arguments(&cmdline),
            env: unix::pick_env(nul_separated(&environ), env_keys),
        })
    }

    /// Memory is the proportional set size, so shared pages are not counted
    /// twice when the engine sums a process tree. Other users' processes only
    /// expose `status`, so they report resident size instead.
    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        let stat = Stat::read(pid)?;
        let memory_kb = fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
            .ok()
            .and_then(|smaps| kb_field(&smaps, "Pss:"))
            .or_else(|| {
                let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
                kb_field(&status, "VmRSS:")
            })
            .unwrap_or(0);
        Some(ProcUsage {
            memory: memory_kb * 1024,
            cpu_time_ns: stat.cpu_ticks * (1_000_000_000 / self.hz),
        })
    }

    /// Used is total minus available, as `free` and most Linux monitors show it.
    fn memory(&self) -> MemoryStats {
        let Ok(meminfo) = fs::read_to_string("/proc/meminfo") else {
            return MemoryStats::default();
        };
        let total = kb_field(&meminfo, "MemTotal:").unwrap_or(0) * 1024;
        let available = kb_field(&meminfo, "MemAvailable:").unwrap_or(0) * 1024;
        MemoryStats {
            total,
            used: total.saturating_sub(available),
        }
    }

    fn cpu_percent(&self) -> f32 {
        let Some(line) = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|stat| stat_line(&stat, "cpu").map(str::to_string))
        else {
            return 0.0;
        };
        // user nice system idle iowait irq softirq steal; guest time is
        // already counted in user and nice.
        let ticks: Vec<u64> = line
            .split_whitespace()
            .take(8)
            .filter_map(|v| v.parse().ok())
            .collect();
        if ticks.len() < 4 {
            return 0.0;
        }
        let total: u64 = ticks.iter().sum();
        let idle = ticks[3] + ticks.get(4).copied().unwrap_or(0);
        let busy = total - idle;
        self.cpu.percent(busy, total)
    }

    fn signal(&self, target: ProcRef, force: bool) -> io::Result<()> {
        unix::signal(target, force, |pid| {
            self.proc_info(pid).map(|p| p.proc.started_at)
        })
    }
}

fn pids() -> io::Result<impl Iterator<Item = u32>> {
    Ok(fs::read_dir("/proc")?.filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok()))
}

/// `socket:[12345]`, the link target of a socket descriptor.
fn socket_inode(link: &std::path::Path) -> Option<u64> {
    link.to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// A row of `/proc/net/tcp` or `tcp6` in LISTEN state.
struct ListenRow {
    address: IpAddr,
    port: u16,
    uid: u32,
    inode: u64,
}

impl ListenRow {
    /// `sl local_address rem_address st ... uid timeout inode`, where the
    /// local address is `ADDR:PORT` in hex. The address is printed as 32-bit
    /// words in host byte order, so each word goes back through native bytes.
    fn parse(row: &str) -> Option<Self> {
        let fields: Vec<&str> = row.split_whitespace().collect();
        if fields.get(3) != Some(&"0A") {
            return None;
        }
        let (address, port) = fields.get(1)?.split_once(':')?;
        let mut bytes = Vec::with_capacity(16);
        for word in address.as_bytes().chunks(8) {
            let word = u32::from_str_radix(std::str::from_utf8(word).ok()?, 16).ok()?;
            bytes.extend_from_slice(&word.to_ne_bytes());
        }
        let address = match bytes.len() {
            4 => IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(bytes).ok()?)),
            16 => IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?)),
            _ => return None,
        };
        Some(Self {
            address,
            port: u16::from_str_radix(port, 16).ok()?,
            uid: fields.get(7)?.parse().ok()?,
            inode: fields.get(9)?.parse().ok()?,
        })
    }
}

/// The fields of `/proc/<pid>/stat` we use.
struct Stat {
    name: String,
    ppid: u32,
    /// utime + stime, in clock ticks.
    cpu_ticks: u64,
    /// Start time in clock ticks after boot.
    start_ticks: u64,
}

impl Stat {
    fn read(pid: u32) -> Option<Self> {
        Self::parse(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
    }

    /// `pid (comm) state ppid ...`. The name may itself contain spaces and
    /// parentheses, so split at the last `)`.
    fn parse(stat: &str) -> Option<Self> {
        let open = stat.find('(')?;
        let close = stat.rfind(')')?;
        let fields: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
        // `fields[0]` is field 3 (state) in proc(5)'s numbering.
        let field = |n: usize| fields.get(n - 3)?.parse::<u64>().ok();
        Some(Self {
            name: stat.get(open + 1..close)?.to_string(),
            ppid: u32::try_from(field(4)?).ok()?,
            cpu_ticks: field(14)? + field(15)?,
            start_ticks: field(22)?,
        })
    }
}

/// The rest of the line in `text` that starts with `key` and a space.
fn stat_line<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
}

/// A `Key:   123 kB` value from files such as `/proc/meminfo`.
fn kb_field(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(key))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// `/proc/<pid>/cmdline`, keeping empty arguments. Trailing NULs are dropped:
/// programs that rewrite their title, such as Node's `process.title`, pad the
/// rest of the original arguments with them.
fn arguments(cmdline: &[u8]) -> Vec<String> {
    let end = cmdline.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    if end == 0 {
        return Vec::new();
    }
    cmdline[..end]
        .split(|&b| b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Splits a NUL-separated block such as `/proc/<pid>/environ` into strings.
fn nul_separated(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
}
