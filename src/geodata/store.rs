//! Where fetched geodata is kept between visits.
//!
//! GDI Berlin answers every request with `Cache-Control: no-store`, so
//! neither the browser nor anything else below the app keeps a byte of it;
//! without this a region would fetch its whole terrain again on every visit.
//! Entries are keyed by request URL - the `geodata` crate's builders emit
//! one canonical string per request - and bounded three ways:
//!
//! - **a time-to-live** ([`TTL_SECS`]): a WMS URL does not change when Berlin
//!   updates its data, so an entry is refetched after a month regardless;
//! - **an epoch** ([`CACHE_EPOCH`]): bumping it discards every entry kept
//!   under an older one, for a change in what a URL's answer means;
//! - **a size cap**, natively ([`MAX_DISK_BYTES`]): least recently used
//!   entries go first. The browser bounds its own storage and evicts under
//!   pressure.
//!
//! Every operation is best-effort: a store that cannot be read is a miss, and
//! one that cannot be written keeps nothing. A visit never fails because the
//! cache did.

#[cfg(test)]
use std::collections::HashMap;
#[cfg(any(not(target_arch = "wasm32"), test))]
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

/// Bump to discard everything kept under the previous value.
pub const CACHE_EPOCH: u32 = 1;

/// How long an entry is trusted: thirty days, in seconds.
pub const TTL_SECS: i64 = 30 * 86_400;

/// The most bytes the native cache directory may hold.
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_DISK_BYTES: u64 = 256 << 20;

/// Where answers are kept.
#[derive(Clone, Debug)]
pub enum GeoStore {
    /// Nowhere: a native run with no cache directory (headless CI).
    Off,
    /// A directory under the platform cache, natively.
    #[cfg(not(target_arch = "wasm32"))]
    Disk(Arc<disk::DiskStore>),
    /// The browser's Cache API, on the web.
    #[cfg(target_arch = "wasm32")]
    Browser,
    /// Process memory, for tests.
    #[cfg(test)]
    Memory(MemoryEntries),
}

/// What the test store keeps: URL -> (stored at, body).
#[cfg(test)]
type MemoryEntries = Arc<Mutex<HashMap<String, (i64, Vec<u8>)>>>;

impl GeoStore {
    /// This platform's store: the browser's Cache API on the web; natively
    /// `geodata/v<epoch>` under `$XDG_CACHE_HOME`, `%LOCALAPPDATA%` or
    /// `~/.cache` (`symbios-overlands/`), or [`GeoStore::Off`] with none.
    pub fn platform_default() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            GeoStore::Browser
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            match disk::platform_root() {
                Some(root) => GeoStore::Disk(Arc::new(disk::DiskStore::new(root, MAX_DISK_BYTES))),
                None => GeoStore::Off,
            }
        }
    }

    /// The body kept for `url`, if one is, is younger than [`TTL_SECS`] at
    /// `now` (Unix seconds), and is at most `cap` bytes.
    pub async fn get(&self, url: &str, cap: usize, now: i64) -> Option<Vec<u8>> {
        match self {
            GeoStore::Off => None,
            #[cfg(not(target_arch = "wasm32"))]
            GeoStore::Disk(disk) => disk.get(url, cap, now),
            #[cfg(target_arch = "wasm32")]
            GeoStore::Browser => {
                let name = browser_cache_name();
                sweep_browser_epochs_once(&name).await;
                let (stored, body) = super::store_browser::read(&name, url).await?;
                if fresh(stored, now) && body.len() <= cap {
                    Some(body)
                } else {
                    super::store_browser::remove(&name, url).await;
                    None
                }
            }
            #[cfg(test)]
            GeoStore::Memory(map) => {
                let map = map.lock().unwrap();
                let (stored, body) = map.get(url)?;
                (fresh(*stored, now) && body.len() <= cap).then(|| body.clone())
            }
        }
    }

    /// Keep `body` for `url`, stamped `now` (Unix seconds).
    pub async fn put(&self, url: &str, body: &[u8], now: i64) {
        match self {
            GeoStore::Off => {}
            #[cfg(not(target_arch = "wasm32"))]
            GeoStore::Disk(disk) => disk.put(url, body, now),
            #[cfg(target_arch = "wasm32")]
            GeoStore::Browser => {
                let name = browser_cache_name();
                sweep_browser_epochs_once(&name).await;
                super::store_browser::write(&name, url, body, now).await;
            }
            #[cfg(test)]
            GeoStore::Memory(map) => {
                map.lock()
                    .unwrap()
                    .insert(url.to_owned(), (now, body.to_vec()));
            }
        }
    }
}

/// Whether an entry stored at `stored` is still trusted at `now`. One that
/// claims to come from more than a day ahead was stamped by a wrong clock
/// and is not.
fn fresh(stored: i64, now: i64) -> bool {
    let age = now.saturating_sub(stored);
    (-86_400..TTL_SECS).contains(&age)
}

/// Every browser cache this app names starts with this; the epoch follows.
#[cfg(target_arch = "wasm32")]
const BROWSER_CACHE_PREFIX: &str = "symbios-overlands-geodata-v";

#[cfg(target_arch = "wasm32")]
fn browser_cache_name() -> String {
    format!("{BROWSER_CACHE_PREFIX}{CACHE_EPOCH}")
}

/// Delete the caches of other epochs, the first time a page uses the store.
#[cfg(target_arch = "wasm32")]
async fn sweep_browser_epochs_once(keep: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SWEPT: AtomicBool = AtomicBool::new(false);
    if !SWEPT.swap(true, Ordering::Relaxed) {
        super::store_browser::sweep(BROWSER_CACHE_PREFIX, keep).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod disk {
    //! The native store: one file per entry, named by a hash of its URL,
    //! holding the URL itself so that two URLs sharing a hash can never be
    //! mistaken for each other.

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use crate::seeded_defaults::hash::fnv1a_64;

    use super::{CACHE_EPOCH, fresh};

    /// Opens every entry file, so a stray file is never read as one.
    const MAGIC: &[u8; 4] = b"SOGD";
    /// The entry layout's version: magic, this byte, stored-at (i64 LE),
    /// the body's FNV-1a hash (u64 LE), URL length (u32 LE), URL, body.
    const FORMAT: u8 = 1;
    const HEADER: usize = 4 + 1 + 8 + 8 + 4;

    /// `symbios-overlands/geodata` under the platform cache base:
    /// `$XDG_CACHE_HOME`, then `%LOCALAPPDATA%`, then `~/.cache`. A base
    /// that is set but empty or relative is ignored, as the XDG spec says -
    /// it would put the cache wherever the app was started.
    pub(crate) fn platform_root() -> Option<PathBuf> {
        root_from(|var| std::env::var_os(var))
    }

    /// [`platform_root`] over any environment, so tests need not edit the
    /// process's own.
    pub(super) fn root_from(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
        let absolute = |var: &str| {
            env(var)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let base = absolute("XDG_CACHE_HOME")
            .or_else(|| absolute("LOCALAPPDATA"))
            .or_else(|| absolute("HOME").map(|home| home.join(".cache")))?;
        Some(base.join("symbios-overlands").join("geodata"))
    }

    /// FNV-1a over bytes: the body checksum an entry carries, so a torn or
    /// damaged file is read as no entry rather than as a shorter answer.
    fn body_hash(body: &[u8]) -> u64 {
        body.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// A cache directory: `<root>/v<epoch>`, holding at most `max_bytes`.
    #[derive(Debug)]
    pub struct DiskStore {
        root: PathBuf,
        dir: PathBuf,
        max_bytes: u64,
        /// Whether older epochs' directories have been cleared this run.
        swept: AtomicBool,
    }

    impl DiskStore {
        /// A store under `root`; nothing is touched until it is used.
        pub fn new(root: PathBuf, max_bytes: u64) -> Self {
            let dir = root.join(format!("v{CACHE_EPOCH}"));
            DiskStore {
                root,
                dir,
                max_bytes,
                swept: AtomicBool::new(false),
            }
        }

        pub(super) fn path_for(&self, url: &str) -> PathBuf {
            self.dir.join(format!("{:016x}.bin", fnv1a_64(url)))
        }

        pub(super) fn get(&self, url: &str, cap: usize, now: i64) -> Option<Vec<u8>> {
            self.sweep_old_epochs();
            let path = self.path_for(url);
            // Too big to be this URL's answer within its cap: not read at all.
            let len = std::fs::metadata(&path).ok()?.len();
            if len > (HEADER + url.len() + cap) as u64 {
                let _ = std::fs::remove_file(&path);
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            match read_entry(&bytes, url, cap, now) {
                Entry::Hit(body) => {
                    // Recently used, for the eviction order. Best-effort.
                    if let Ok(file) = std::fs::File::options().append(true).open(&path) {
                        let _ = file.set_modified(std::time::SystemTime::now());
                    }
                    Some(body.to_vec())
                }
                // Another URL with the same hash: its entry is valid, and the
                // next put of this URL replaces it.
                Entry::OtherUrl => None,
                Entry::Stale | Entry::Corrupt => {
                    let _ = std::fs::remove_file(&path);
                    None
                }
            }
        }

        pub(super) fn put(&self, url: &str, body: &[u8], now: i64) {
            self.sweep_old_epochs();
            if std::fs::create_dir_all(&self.dir).is_err() {
                return;
            }
            let path = self.path_for(url);
            let tmp = temp_path(&path);
            if std::fs::write(&tmp, encode_entry(url, body, now)).is_err()
                || std::fs::rename(&tmp, &path).is_err()
            {
                let _ = std::fs::remove_file(&tmp);
                return;
            }
            self.enforce_cap();
        }

        /// Delete least recently used entries until the directory holds at
        /// most three quarters of the cap, once it holds more than the cap.
        /// The slack means a full cache is swept now and then, not on every
        /// write. Only this store's own files - entries and the temporary
        /// files of writes - are counted or deleted.
        fn enforce_cap(&self) {
            let Ok(entries) = std::fs::read_dir(&self.dir) else {
                return;
            };
            let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.ends_with(".bin") || name.ends_with(".tmp")
                })
                .filter_map(|entry| {
                    let meta = entry.metadata().ok().filter(|m| m.is_file())?;
                    Some((meta.modified().ok()?, meta.len(), entry.path()))
                })
                .collect();
            let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
            if total <= self.max_bytes {
                return;
            }
            files.sort();
            let target = self.max_bytes / 4 * 3;
            for (_, len, path) in files {
                if total <= target {
                    break;
                }
                if std::fs::remove_file(&path).is_ok() {
                    total -= len;
                }
            }
        }

        /// Remove the directories of older (or newer) epochs, once a run.
        fn sweep_old_epochs(&self) {
            if self.swept.swap(true, Ordering::Relaxed) {
                return;
            }
            let Ok(entries) = std::fs::read_dir(&self.root) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path != self.dir && is_epoch_dir(&path) {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }
    }

    /// A temporary file for one write of `path`, never shared with another
    /// write - of this process or any other. Writing through it and renaming
    /// it over `path` means a crash, two tasks of this process or a second
    /// copy of the app writing one entry leave the old entry or one whole new
    /// one, never parts of two (#1582: a per-process name tore entries when
    /// a cleared fetch and its successor wrote the same URL at once).
    pub(super) fn temp_path(path: &Path) -> PathBuf {
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        PathBuf::from(tmp)
    }

    /// `v<digits>`: a directory this store made, under any epoch.
    fn is_epoch_dir(path: &Path) -> bool {
        path.is_dir()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix('v'))
                .is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                })
    }

    pub(super) fn encode_entry(url: &str, body: &[u8], now: i64) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + url.len() + body.len());
        out.extend_from_slice(MAGIC);
        out.push(FORMAT);
        out.extend_from_slice(&now.to_le_bytes());
        out.extend_from_slice(&body_hash(body).to_le_bytes());
        out.extend_from_slice(&(url.len() as u32).to_le_bytes());
        out.extend_from_slice(url.as_bytes());
        out.extend_from_slice(body);
        out
    }

    /// What an entry file holds, for `url` at `now`.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum Entry<'a> {
        /// This URL's body, fresh and within the cap.
        Hit(&'a [u8]),
        /// A valid entry for another URL with the same hash.
        OtherUrl,
        /// This URL's entry, but past its time-to-live (or over the cap).
        Stale,
        /// Not an entry this store wrote whole: a stray file, a torn or
        /// damaged one (its body does not match its checksum).
        Corrupt,
    }

    pub(super) fn read_entry<'a>(bytes: &'a [u8], url: &str, cap: usize, now: i64) -> Entry<'a> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[4] != FORMAT {
            return Entry::Corrupt;
        }
        let stored = i64::from_le_bytes(bytes[5..13].try_into().expect("8 bytes"));
        let hash = u64::from_le_bytes(bytes[13..21].try_into().expect("8 bytes"));
        let url_len = u32::from_le_bytes(bytes[21..25].try_into().expect("4 bytes")) as usize;
        let Some(stored_url) = bytes.get(HEADER..HEADER.saturating_add(url_len)) else {
            return Entry::Corrupt;
        };
        let body = &bytes[HEADER + url_len..];
        if body_hash(body) != hash {
            return Entry::Corrupt;
        }
        if stored_url != url.as_bytes() {
            return Entry::OtherUrl;
        }
        if !fresh(stored, now) || body.len() > cap {
            return Entry::Stale;
        }
        Entry::Hit(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh directory per test, under the system temp dir.
    fn scratch_dir(name: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "overlands-geodata-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn block<F: std::future::Future>(f: F) -> F::Output {
        futures_lite::future::block_on(f)
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn freshness_spans_the_ttl_and_refuses_the_future() {
        assert!(fresh(NOW, NOW));
        assert!(fresh(NOW - TTL_SECS + 1, NOW));
        assert!(!fresh(NOW - TTL_SECS, NOW));
        assert!(fresh(NOW + 3_600, NOW), "an hour of clock skew is fine");
        assert!(!fresh(NOW + 2 * 86_400, NOW));
        assert!(!fresh(i64::MIN, NOW));
        assert!(!fresh(i64::MAX, NOW));
    }

    #[test]
    fn disk_entries_round_trip_and_expire() {
        let root = scratch_dir("round-trip");
        let store = GeoStore::Disk(Arc::new(disk::DiskStore::new(root.clone(), MAX_DISK_BYTES)));
        let url = "https://gdi.berlin.de/services/wms/dgm1?a=1";
        assert_eq!(block(store.get(url, 100, NOW)), None);
        block(store.put(url, b"body", NOW));
        assert_eq!(block(store.get(url, 100, NOW + 10)), Some(b"body".to_vec()));
        assert_eq!(block(store.get(url, 3, NOW)), None, "over the cap");
        assert_eq!(
            block(store.get("https://gdi.berlin.de/other", 100, NOW)),
            None
        );
        assert_eq!(block(store.get(url, 100, NOW + TTL_SECS)), None, "expired");
        // The expired entry was removed, not just skipped.
        assert_eq!(block(store.get(url, 100, NOW)), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn entries_name_their_url_and_refuse_anything_else() {
        let url = "https://gdi.berlin.de/services/wms/dgm1?a=1";
        let entry = disk::encode_entry(url, b"png", NOW);
        assert_eq!(
            disk::read_entry(&entry, url, 10, NOW),
            disk::Entry::Hit(b"png")
        );
        assert_eq!(
            disk::read_entry(
                &entry,
                "https://gdi.berlin.de/services/wms/dgm1?a=2",
                10,
                NOW
            ),
            disk::Entry::OtherUrl
        );
        assert_eq!(disk::read_entry(&entry, url, 2, NOW), disk::Entry::Stale);
        assert_eq!(
            disk::read_entry(&entry[..10], url, 10, NOW),
            disk::Entry::Corrupt
        );
        let mut lying = entry.clone();
        lying[21..25].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(disk::read_entry(&lying, url, 10, NOW), disk::Entry::Corrupt);
        let mut torn = entry.clone();
        *torn.last_mut().unwrap() ^= 0xFF;
        assert_eq!(
            disk::read_entry(&torn, url, 10, NOW),
            disk::Entry::Corrupt,
            "checksum"
        );
        let truncated = &entry[..entry.len() - 1];
        assert_eq!(
            disk::read_entry(truncated, url, 10, NOW),
            disk::Entry::Corrupt
        );
        let mut other_format = entry;
        other_format[4] = 2;
        assert_eq!(
            disk::read_entry(&other_format, url, 10, NOW),
            disk::Entry::Corrupt
        );
        assert_eq!(
            disk::read_entry(b"not an entry at all", url, 10, NOW),
            disk::Entry::Corrupt
        );
    }

    #[test]
    fn a_full_disk_cache_drops_its_least_recently_used_entries() {
        let root = scratch_dir("evict");
        // Room for about three 1000-byte bodies with their headers.
        let disk = Arc::new(disk::DiskStore::new(root.clone(), 3_300));
        let store = GeoStore::Disk(disk);
        let url = |k: u32| format!("https://gdi.berlin.de/services/wms/x?k={k}");
        let body = vec![7u8; 1000];
        // Written minutes apart, by their mtimes - set outright, so the order
        // holds on a filesystem with coarse timestamps too.
        let GeoStore::Disk(disk) = &store else {
            unreachable!()
        };
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        for k in 0..3 {
            block(store.put(&url(k), &body, NOW));
            let file = std::fs::File::options()
                .append(true)
                .open(disk.path_for(&url(k)))
                .unwrap();
            file.set_modified(past + std::time::Duration::from_secs(60 * u64::from(k)))
                .unwrap();
        }
        // Touch the oldest, so the second is now least recently used.
        assert!(block(store.get(&url(0), 2000, NOW)).is_some());
        block(store.put(&url(3), &body, NOW));
        assert!(
            block(store.get(&url(1), 2000, NOW)).is_none(),
            "LRU entry evicted"
        );
        assert!(
            block(store.get(&url(0), 2000, NOW)).is_some(),
            "the touched entry stays"
        );
        assert!(
            block(store.get(&url(3), 2000, NOW)).is_some(),
            "the new entry stays"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn older_epochs_are_cleared_once_and_nothing_else_is() {
        let root = scratch_dir("epochs");
        std::fs::create_dir_all(root.join("v0")).unwrap();
        std::fs::write(root.join("v0").join("a.bin"), b"old").unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(root.join("keep.txt"), b"x").unwrap();
        let store = GeoStore::Disk(Arc::new(disk::DiskStore::new(root.clone(), MAX_DISK_BYTES)));
        block(store.put("https://gdi.berlin.de/services/x", b"new", NOW));
        assert!(!root.join("v0").exists(), "the old epoch is gone");
        assert!(root.join("notes").exists() && root.join("keep.txt").exists());
        assert!(root.join(format!("v{CACHE_EPOCH}")).is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_unwritable_store_keeps_nothing_and_fails_nothing() {
        // A file where the directory should be: every write fails.
        let root = scratch_dir("unwritable");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(&root, b"a file").unwrap();
        let store = GeoStore::Disk(Arc::new(disk::DiskStore::new(root.clone(), MAX_DISK_BYTES)));
        block(store.put("https://gdi.berlin.de/services/x", b"body", NOW));
        assert_eq!(
            block(store.get("https://gdi.berlin.de/services/x", 100, NOW)),
            None
        );
        let _ = std::fs::remove_file(root);
    }

    #[test]
    fn the_cache_base_must_be_absolute() {
        use std::ffi::OsString;
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |var: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == var)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let root = |pairs| disk::root_from(env(pairs));
        assert_eq!(
            root(&[("XDG_CACHE_HOME", "/var/cache/u"), ("HOME", "/home/u")]),
            Some(PathBuf::from("/var/cache/u/symbios-overlands/geodata"))
        );
        // Set but empty, or relative: ignored, the next base is used.
        assert_eq!(
            root(&[("XDG_CACHE_HOME", ""), ("HOME", "/home/u")]),
            Some(PathBuf::from("/home/u/.cache/symbios-overlands/geodata"))
        );
        assert_eq!(
            root(&[("XDG_CACHE_HOME", "cache"), ("HOME", "/home/u")]),
            Some(PathBuf::from("/home/u/.cache/symbios-overlands/geodata"))
        );
        assert_eq!(root(&[("HOME", "relative")]), None);
        assert_eq!(root(&[]), None);
    }

    #[test]
    fn only_the_stores_own_files_are_read_counted_or_evicted() {
        let root = scratch_dir("own-files");
        let disk = Arc::new(disk::DiskStore::new(root.clone(), 3_000));
        let store = GeoStore::Disk(disk.clone());
        let dir = root.join(format!("v{CACHE_EPOCH}"));
        std::fs::create_dir_all(&dir).unwrap();
        // Somebody's file, bigger than the whole cap: never evicted.
        std::fs::write(dir.join("notes.txt"), vec![b'x'; 10_000]).unwrap();
        let url = "https://gdi.berlin.de/services/wms/x?k=1";
        block(store.put(url, &[1u8; 1000], NOW));
        assert!(dir.join("notes.txt").exists());
        assert!(block(store.get(url, 2000, NOW)).is_some());
        // An entry file far past its URL's cap is refused unread, and removed.
        std::fs::write(disk.path_for(url), vec![0u8; 50_000]).unwrap();
        assert_eq!(block(store.get(url, 2000, NOW)), None);
        assert!(!disk.path_for(url).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_write_has_its_own_temporary_file() {
        let entry = PathBuf::from("/cache/v1/0123456789abcdef.bin");
        let (first, second) = (disk::temp_path(&entry), disk::temp_path(&entry));
        assert_ne!(first, second);
        for tmp in [first, second] {
            assert_eq!(tmp.parent(), entry.parent(), "renamed within one directory");
            let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with("0123456789abcdef.bin.") && name.ends_with(".tmp"));
        }
    }

    #[test]
    fn concurrent_writers_of_one_entry_never_leave_parts_of_two() {
        let root = scratch_dir("writers");
        let store = GeoStore::Disk(Arc::new(disk::DiskStore::new(root.clone(), MAX_DISK_BYTES)));
        let url = "https://gdi.berlin.de/services/wms/dgm1?shared";
        let a = vec![b'a'; 300_000];
        let b = vec![b'b'; 100_000];
        std::thread::scope(|scope| {
            for body in [&a, &b] {
                let store = store.clone();
                scope.spawn(move || {
                    for _ in 0..40 {
                        block(store.put(url, body, NOW));
                    }
                });
            }
            // Once written, the entry is always one whole body: a rename
            // replaces it atomically, so it can only go missing if a torn
            // write failed its checksum and was discarded.
            let mut seen = false;
            for _ in 0..400 {
                match block(store.get(url, 1 << 20, NOW)) {
                    Some(read) => {
                        assert!(read == a || read == b, "{} bytes of neither", read.len());
                        seen = true;
                    }
                    None => assert!(!seen, "the entry went missing: a torn write"),
                }
            }
        });
        let last = block(store.get(url, 1 << 20, NOW)).unwrap();
        assert!(last == a || last == b);
        let leftovers = std::fs::read_dir(root.join(format!("v{CACHE_EPOCH}")))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0, "every temporary file was renamed or removed");
        let _ = std::fs::remove_dir_all(root);
    }
}
