//! A bounded on-disk cache of byte blobs (library thumbnails).
//!
//! Entries are files named by a 64-bit key, sharded into 256 subdirectories. The cache
//! is disposable: a missing, unreadable or half-written entry is a miss, and the
//! directory can be deleted at any time.
//!
//! Recency is the file's modification time: a hit refreshes it (at most once per
//! [`TOUCH_INTERVAL`]), and eviction removes the least recently used files until the
//! total is back under 90% of the budget. Usage is counted from a directory scan on
//! the first write, then tracked in memory.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Hits refresh an entry's recency at most this often (saves a write per hit).
pub const TOUCH_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Temporary files older than this are left over from a crash and are removed.
const STALE_TEMP: Duration = Duration::from_secs(60 * 60);
const TEMP_SUFFIX: &str = ".tmp";

pub struct DiskCache {
    dir: PathBuf,
    extension: &'static str,
    budget_bytes: u64,
    /// Bytes on disk; `None` until the first write scans the directory.
    used: Mutex<Option<u64>>,
    next_temp: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskCacheStats {
    pub entries: usize,
    pub bytes: u64,
    pub budget_bytes: u64,
}

impl DiskCache {
    /// A cache in `dir` (created on first write) holding files named `<key>.<extension>`.
    /// Does no I/O.
    pub fn new(dir: impl Into<PathBuf>, extension: &'static str, budget_bytes: u64) -> Self {
        Self {
            dir: dir.into(),
            extension,
            budget_bytes,
            used: Mutex::new(None),
            next_temp: AtomicU64::new(0),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The entry for `key`, if present and readable.
    pub fn get(&self, key: u64) -> Option<Vec<u8>> {
        // Opened for writing too: refreshing the time needs write access on Windows.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.path(key))
            .ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        let now = SystemTime::now();
        let stale = file
            .metadata()
            .and_then(|m| m.modified())
            .map_or(true, |t| {
                now.duration_since(t).unwrap_or_default() > TOUCH_INTERVAL
            });
        if stale {
            let _ = file.set_modified(now);
        }
        Some(bytes)
    }

    /// Stores `bytes` under `key`, replacing any previous entry, then evicts if over
    /// budget. The write is atomic (temporary file, then rename), so a reader never
    /// sees a partial entry. Not synced to disk: after a power loss an entry may be
    /// empty or missing, which callers treat as a miss.
    pub fn put(&self, key: u64, bytes: &[u8]) -> io::Result<()> {
        let dest = self.path(key);
        let shard = dest.parent().expect("entries live in a shard directory");
        fs::create_dir_all(shard)?;
        let temp = shard.join(format!(
            "{}.{}-{}{TEMP_SUFFIX}",
            key_name(key),
            std::process::id(),
            self.next_temp.fetch_add(1, Ordering::Relaxed)
        ));
        let replaced = fs::metadata(&dest).map_or(0, |m| m.len());
        let written = File::create(&temp)
            .and_then(|mut f| f.write_all(bytes))
            .and_then(|()| fs::rename(&temp, &dest));
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(e);
        }

        let mut used = self.used.lock().expect("disk cache lock");
        let total = match *used {
            Some(n) => n.saturating_sub(replaced) + bytes.len() as u64,
            // First write this session: count what is already there (this entry included).
            None => self.stats().bytes,
        };
        *used = Some(if total > self.budget_bytes {
            self.evict()
        } else {
            total
        });
        Ok(())
    }

    /// Removes the entry for `key` (e.g. found to be corrupt).
    pub fn remove(&self, key: u64) {
        let path = self.path(key);
        let len = fs::metadata(&path).map_or(0, |m| m.len());
        if fs::remove_file(&path).is_ok()
            && let Some(n) = self.used.lock().expect("disk cache lock").as_mut()
        {
            *n = n.saturating_sub(len);
        }
    }

    /// Entries and bytes on disk (scans the directory).
    pub fn stats(&self) -> DiskCacheStats {
        let entries = self.entries();
        DiskCacheStats {
            entries: entries.len(),
            bytes: entries.iter().map(|e| e.len).sum(),
            budget_bytes: self.budget_bytes,
        }
    }

    fn path(&self, key: u64) -> PathBuf {
        let name = key_name(key);
        self.dir
            .join(&name[..2])
            .join(format!("{name}.{}", self.extension))
    }

    /// Deletes least recently used entries until the total is under 90% of the
    /// budget; returns the bytes left.
    fn evict(&self) -> u64 {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|e| e.len).sum();
        let target = self.budget_bytes / 10 * 9;
        entries.sort_by_key(|e| e.modified);
        for e in entries {
            if total <= target {
                break;
            }
            if fs::remove_file(&e.path).is_ok() {
                total -= e.len;
            }
        }
        total
    }

    /// Current entries. Stale temporary files and anything else foreign found in
    /// the shards are removed along the way.
    fn entries(&self) -> Vec<Entry> {
        let suffix = format!(".{}", self.extension);
        let now = SystemTime::now();
        let mut out = Vec::new();
        let Ok(shards) = fs::read_dir(&self.dir) else {
            return out;
        };
        for shard in shards.flatten() {
            let Ok(files) = fs::read_dir(shard.path()) else {
                continue;
            };
            for file in files.flatten() {
                let Ok(meta) = file.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let name = file.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(&suffix) {
                    out.push(Entry {
                        path: file.path(),
                        len: meta.len(),
                        modified,
                    });
                } else if now.duration_since(modified).unwrap_or_default() > STALE_TEMP {
                    let _ = fs::remove_file(file.path());
                }
            }
        }
        out
    }
}

struct Entry {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

fn key_name(key: u64) -> String {
    format!("{key:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "pe-disk-cache-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&p);
            Self(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn age(cache: &DiskCache, key: u64, secs: u64) {
        let f = OpenOptions::new()
            .write(true)
            .open(cache.path(key))
            .unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(secs))
            .unwrap();
    }

    #[test]
    fn stores_and_returns_entries() {
        let dir = Dir::new("roundtrip");
        let cache = DiskCache::new(&dir.0, "jpg", 1 << 20);
        assert_eq!(cache.get(7), None);
        cache.put(7, b"seven").unwrap();
        cache.put(u64::MAX, b"max").unwrap();
        assert_eq!(cache.get(7).as_deref(), Some(&b"seven"[..]));
        assert_eq!(cache.get(u64::MAX).as_deref(), Some(&b"max"[..]));
        cache.put(7, b"SEVEN!").unwrap();
        assert_eq!(cache.get(7).as_deref(), Some(&b"SEVEN!"[..]));
        let stats = cache.stats();
        assert_eq!((stats.entries, stats.bytes), (2, 9));
        cache.remove(7);
        assert_eq!(cache.get(7), None);
        assert!(cache.path(1).ends_with("00/0000000000000001.jpg"));
    }

    #[test]
    fn evicts_least_recently_used_to_stay_under_budget() {
        let dir = Dir::new("evict");
        let cache = DiskCache::new(&dir.0, "bin", 1000);
        for key in 0..4 {
            cache.put(key, &[0; 240]).unwrap();
            age(&cache, key, 10_000 - key * 100); // key 0 is the oldest
        }
        // Using key 0 makes it the most recent; key 1 is now the oldest.
        assert!(cache.get(0).is_some());
        cache.put(4, &[0; 240]).unwrap(); // 1200 > 1000: evict down to <= 900
        let kept: Vec<u64> = (0..5).filter(|k| cache.get(*k).is_some()).collect();
        assert_eq!(kept, [0, 3, 4]);
        assert!(cache.stats().bytes <= 900);
    }

    #[test]
    fn usage_survives_reopening_and_stale_temp_files_are_removed() {
        let dir = Dir::new("reopen");
        {
            let cache = DiskCache::new(&dir.0, "bin", 1000);
            cache.put(1, &[0; 600]).unwrap();
        }
        let stray = dir.0.join("00").join("0000000000000001.99-0.tmp");
        fs::write(&stray, b"half").unwrap();
        File::options()
            .write(true)
            .open(&stray)
            .unwrap()
            .set_modified(SystemTime::now() - 2 * STALE_TEMP)
            .unwrap();
        let cache = DiskCache::new(&dir.0, "bin", 1000);
        // The first write counts the existing 600 bytes, so this one must evict.
        cache.put(2, &[0; 600]).unwrap();
        assert_eq!(cache.get(1), None);
        assert!(cache.get(2).is_some());
        assert!(!stray.exists());
    }

    #[test]
    fn hits_refresh_recency_only_when_stale() {
        let dir = Dir::new("touch");
        let cache = DiskCache::new(&dir.0, "bin", 1 << 20);
        cache.put(9, b"x").unwrap();
        let modified = |c: &DiskCache| fs::metadata(c.path(9)).unwrap().modified().unwrap();
        age(&cache, 9, 60);
        let recent = modified(&cache);
        cache.get(9).unwrap();
        assert_eq!(modified(&cache), recent, "recent entries are not rewritten");
        age(&cache, 9, TOUCH_INTERVAL.as_secs() * 2);
        cache.get(9).unwrap();
        let age_now = SystemTime::now().duration_since(modified(&cache)).unwrap();
        assert!(age_now < Duration::from_secs(60));
    }

    #[test]
    fn missing_directory_is_just_empty() {
        let dir = Dir::new("missing");
        let cache = DiskCache::new(dir.0.join("never-created"), "bin", 100);
        assert_eq!(cache.get(1), None);
        assert_eq!(cache.stats().entries, 0);
        cache.remove(1);
    }
}
