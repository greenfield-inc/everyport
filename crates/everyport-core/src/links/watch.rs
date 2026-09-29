//! Cached results that are read again only when a file they came from changes.
//!
//! Each cache entry lists the files its value was read from. A `notify` watch
//! on each file's folder (non-recursive) marks the entry stale, with events
//! batched for 1.5 s. A 60 s tick `stat`s the watched files to catch events
//! the OS coalesced or dropped, and is the only check when watching fails.
//! The same tick drops entries unused for 60 s, and their watches. A fresh
//! hit touches no file, not even with `stat`.

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime};

const BATCH: Duration = Duration::from_millis(1500);
const TICK: Duration = Duration::from_secs(60);
/// Entries no scan asked for in this long belong to servers that are gone.
pub(super) const UNUSED_FOR: Duration = Duration::from_secs(60);

static FILES: LazyLock<Files> = LazyLock::new(Files::start);

/// A cache whose entries stay fresh until one of their files changes.
pub struct Cache<K, T>(Arc<Entries<K, T>>);

struct Entries<K, T>(Mutex<HashMap<K, Entry<T>>>);

struct Entry<T> {
    value: T,
    sources: Vec<(PathBuf, u64)>,
    used: Instant,
}

impl<K, T> Default for Cache<K, T>
where
    K: Hash + Eq + Clone + Send + 'static,
    T: Clone + Send + 'static,
{
    fn default() -> Self {
        let entries = Arc::new(Entries(Mutex::default()));
        let weak: Weak<dyn Sweep> = Arc::downgrade(&entries) as Weak<dyn Sweep>;
        FILES.caches.lock().unwrap().push(weak);
        Self(entries)
    }
}

impl<K: Hash + Eq + Clone, T: Clone> Cache<K, T> {
    /// The cached value while its files are unchanged. Otherwise `read` runs,
    /// given the stale value if there is one, and returns the new value with
    /// the files it was read from.
    pub fn get(&self, key: &K, read: impl FnOnce(Option<T>) -> (Vec<PathBuf>, T)) -> T {
        let now = Instant::now();
        let stale = {
            let mut entries = self.0 .0.lock().unwrap();
            match entries.get_mut(key) {
                Some(entry) if FILES.unchanged(&entry.sources) => {
                    entry.used = now;
                    return entry.value.clone();
                }
                entry => entry.map(|e| e.value.clone()),
            }
        };
        let (files, value) = read(stale);
        let sources = FILES.track(files);
        let entry = Entry {
            value: value.clone(),
            sources,
            used: now,
        };
        if let Some(old) = self.0 .0.lock().unwrap().insert(key.clone(), entry) {
            FILES.untrack(&old.sources);
        }
        value
    }
}

trait Sweep: Send + Sync {
    fn sweep(&self, now: Instant);
}

impl<K: Send, T: Send> Sweep for Entries<K, T> {
    fn sweep(&self, now: Instant) {
        self.0.lock().unwrap().retain(|_, entry| {
            let keep = now.duration_since(entry.used) < UNUSED_FOR;
            if !keep {
                FILES.untrack(&entry.sources);
            }
            keep
        });
    }
}

// ---------------------------------------------------------------- watched files

struct Files {
    state: Mutex<State>,
    /// None when the OS watcher couldn't start; the tick covers everything then.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Asks the loop for a `stat` check. Changing the watch set restarts the
    /// FSEvents stream on macOS, which drops events in flight.
    recheck: mpsc::Sender<notify::Result<notify::Event>>,
    caches: Mutex<Vec<Weak<dyn Sweep>>>,
}

/// What `recheck` sends; the loop handles any error as "stat everything".
fn recheck() -> notify::Result<notify::Event> {
    Err(notify::Error::generic("watches changed"))
}

#[derive(Default)]
struct State {
    files: HashMap<PathBuf, Tracked>,
    /// Watched folders: how many tracked files each holds, and its real path.
    folders: HashMap<PathBuf, (usize, PathBuf)>,
    /// Real path back to the tracked folder, for folders reached through a
    /// symlink. FSEvents reports, and filters by, real paths (`/private/var`
    /// for `/var` on macOS), so the real path is what gets watched.
    real_folders: HashMap<PathBuf, PathBuf>,
}

struct Tracked {
    /// Bumped on every change; entries remember the version they read.
    version: u64,
    stamp: Option<Stamp>,
    refs: usize,
}

/// What the tick compares: modified time and size.
type Stamp = (SystemTime, u64);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

impl Files {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let recheck = tx.clone();
        let watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .ok();
        std::thread::spawn(move || watch_loop(rx));
        Self {
            state: Mutex::default(),
            watcher: Mutex::new(watcher),
            recheck,
            caches: Mutex::default(),
        }
    }

    fn unchanged(&self, sources: &[(PathBuf, u64)]) -> bool {
        let state = self.state.lock().unwrap();
        sources
            .iter()
            .all(|(path, version)| state.files.get(path).is_some_and(|f| f.version == *version))
    }

    /// Starts watching `files` (once per file) and returns their versions.
    fn track(&self, files: Vec<PathBuf>) -> Vec<(PathBuf, u64)> {
        let mut state = self.state.lock().unwrap();
        let mut new_folders = Vec::new();
        let sources = files
            .into_iter()
            .map(|path| {
                let file = state.files.entry(path.clone()).or_insert_with(|| {
                    if let Some(folder) = path.parent() {
                        new_folders.push(folder.to_path_buf());
                    }
                    Tracked {
                        version: 0,
                        stamp: stamp(&path),
                        refs: 0,
                    }
                });
                file.refs += 1;
                let version = file.version;
                (path, version)
            })
            .collect();
        for folder in new_folders {
            if let Some((count, _)) = state.folders.get_mut(&folder) {
                *count += 1;
                continue;
            }
            let real = folder.canonicalize().unwrap_or_else(|_| folder.clone());
            if real != folder {
                state.real_folders.insert(real.clone(), folder.clone());
            }
            if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
                // A folder that can't be watched (missing, handle limits,
                // WSL /mnt) is left to the tick.
                let _ = watcher.watch(&real, RecursiveMode::NonRecursive);
            }
            state.folders.insert(folder, (1, real));
            let _ = self.recheck.send(recheck());
        }
        sources
    }

    fn untrack(&self, sources: &[(PathBuf, u64)]) {
        let mut state = self.state.lock().unwrap();
        for (path, _) in sources {
            let Some(file) = state.files.get_mut(path) else {
                continue;
            };
            file.refs -= 1;
            if file.refs > 0 {
                continue;
            }
            state.files.remove(path);
            let Some(folder) = path.parent() else {
                continue;
            };
            let Some((count, _)) = state.folders.get_mut(folder) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                let (_, real) = state.folders.remove(folder).unwrap();
                state.real_folders.remove(&real);
                if let Some(watcher) = self.watcher.lock().unwrap().as_mut() {
                    let _ = watcher.unwatch(&real);
                }
                let _ = self.recheck.send(recheck());
            }
        }
    }

    /// Marks the tracked files among `paths` (from OS events) changed.
    fn changed(&self, paths: HashSet<PathBuf>) {
        let mut state = self.state.lock().unwrap();
        for path in paths {
            let path = match (path.parent(), path.file_name()) {
                (Some(folder), Some(name)) if !state.files.contains_key(&path) => {
                    match state.real_folders.get(folder) {
                        Some(watched) => watched.join(name),
                        None => continue,
                    }
                }
                _ => path,
            };
            if let Some(file) = state.files.get_mut(&path) {
                file.version += 1;
                file.stamp = stamp(&path);
            }
        }
    }

    /// `stat`s every tracked file and marks the ones whose stamp moved.
    fn tick(&self) {
        let paths: Vec<PathBuf> = self.state.lock().unwrap().files.keys().cloned().collect();
        // `stat` outside the lock, so scans never wait on the disk.
        let stamps: Vec<_> = paths.into_iter().map(|p| (stamp(&p), p)).collect();
        let mut state = self.state.lock().unwrap();
        for (now, path) in stamps {
            if let Some(file) = state.files.get_mut(&path) {
                if file.stamp != now {
                    file.stamp = now;
                    file.version += 1;
                }
            }
        }
    }

    fn sweep(&self) {
        let now = Instant::now();
        let caches: Vec<_> = {
            let mut caches = self.caches.lock().unwrap();
            caches.retain(|c| c.strong_count() > 0);
            caches.iter().filter_map(Weak::upgrade).collect()
        };
        for cache in caches {
            cache.sweep(now);
        }
    }
}

/// Batches events for 1.5 s, then marks the files they name changed, or
/// `stat`s every file after an error, an OS rescan request or a watch change.
/// Ticks every 60 s.
fn watch_loop(rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    let mut next_tick = Instant::now() + TICK;
    loop {
        match rx.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(event) => {
                let mut paths = HashSet::new();
                let mut rescan = false;
                let batch_end = Instant::now() + BATCH;
                let mut next = Some(event);
                while let Some(event) = next {
                    match event {
                        Ok(event) => {
                            rescan |= event.need_rescan();
                            paths.extend(event.paths);
                        }
                        Err(_) => rescan = true,
                    }
                    next = rx
                        .recv_timeout(batch_end.saturating_duration_since(Instant::now()))
                        .ok();
                }
                if rescan {
                    FILES.tick();
                } else {
                    FILES.changed(paths);
                }
            }
            // `Files` holds a sender, so the channel stays open.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                FILES.tick();
                FILES.sweep();
                next_tick = Instant::now() + TICK;
            }
        }
    }
}
