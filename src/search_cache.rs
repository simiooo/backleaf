//! Completed directory and indexed-query results. Session-local disk LRU; the persistent index
//! supplies transactional changed IDs. No partially written result can be reused.
use crate::{file_view::FileView, operations::windows::identity};
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_ENTRIES: usize = 8;
pub const MAX_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_AGE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    filesystem: bool,
    identity: Vec<u8>,
    pub revision: i64,
    pub sequence: i64,
    floor: i64,
    roots: Vec<(i64, Vec<u8>)>,
    pub healthy: bool,
    day: chrono::NaiveDate,
}
impl Stamp {
    pub fn date_is_current(&self) -> bool {
        self.day == chrono::Local::now().date_naive()
    }
    pub fn read(db: &Connection, directory: &Path) -> anyhow::Result<Self> {
        // A short read transaction makes health, coverage and watermarks agree.
        let txn = db.unchecked_transaction()?;
        let revision = txn.query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
            r.get(0)
        })?;
        let sequence = txn.query_row(
            "SELECT coalesce((SELECT seq FROM sqlite_sequence WHERE name='search_changes'),0)",
            [],
            |r| r.get(0),
        )?;
        let floor = txn.query_row("SELECT coalesce(min(seq),0) FROM search_changes", [], |r| {
            r.get(0)
        })?;
        let healthy = txn.query_row("SELECT NOT EXISTS(SELECT 1 FROM frontier) AND NOT EXISTS(SELECT 1 FROM problems) AND NOT EXISTS(SELECT 1 FROM roots WHERE building<>0)", [], |r| r.get(0))?;
        let roots = txn
            .prepare("SELECT id,path FROM roots ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        txn.commit()?;
        Ok(Self {
            filesystem: false,
            identity: identity::capture(&crate::index::database(directory))?
                .identity
                .encode()
                .to_vec(),
            revision,
            sequence,
            floor,
            roots,
            healthy,
            day: chrono::Local::now().date_naive(),
        })
    }
    fn follows(&self, previous: &Self) -> bool {
        if self.filesystem || previous.filesystem {
            return self.filesystem
                && previous.filesystem
                && self.healthy
                && previous.healthy
                && self.identity == previous.identity
                && self.revision == previous.revision
                && self.sequence == previous.sequence
                && self.day == previous.day;
        }
        self.healthy
            && previous.healthy
            && self.identity == previous.identity
            && self.roots == previous.roots
            && self.day == previous.day
            && self.sequence >= previous.sequence
            && self.revision >= previous.revision
            && (self.sequence == previous.sequence
                || previous.sequence >= self.floor.saturating_sub(1))
    }
    pub fn changed_ids(&self, db: &Connection, previous: &Self) -> anyhow::Result<Vec<i64>> {
        anyhow::ensure!(self.follows(previous), "缓存变更水位失效");
        let txn = db.unchecked_transaction()?;
        let (head,floor):(i64,i64)=txn.query_row("SELECT coalesce((SELECT seq FROM sqlite_sequence WHERE name='search_changes'),0),coalesce((SELECT min(seq) FROM search_changes),0)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        anyhow::ensure!(
            head == self.sequence && floor == self.floor,
            "索引在准备缓存时已改变"
        );
        let ids=txn.prepare("SELECT DISTINCT entry FROM search_changes WHERE seq>?1 AND seq<=?2 ORDER BY entry LIMIT 8193")?.query_map([previous.sequence,self.sequence], |r| r.get(0))?.collect::<Result<Vec<_>,_>>()?;
        txn.commit()?;
        Ok(ids)
    }
}

pub struct Entry {
    pub key: String,
    pub stamp: Stamp,
    pub file: crate::search::Database,
    pub created: Instant,
    pub skipped: usize,
    pub count: usize,
}
#[derive(Default)]
pub struct Cache {
    entries: VecDeque<Entry>,
    directory: Option<Directory>,
    watches: VecDeque<RootWatch>,
    hit: bool,
}
pub fn key(view: &FileView) -> anyhow::Result<String> {
    let mut view = view.clone();
    view.query = view.query.to_lowercase();
    view.extensions.sort();
    view.extensions.dedup();
    Ok(serde_json::to_string(&view)?)
}
impl Cache {
    /// Whether a fresh completed result with this key follows the given index
    /// stamp. Used to skip the selectivity preflight on a cache hit.
    pub fn contains(&self, key: &str, stamp: &Stamp) -> bool {
        self.entries.iter().any(|entry| {
            entry.key == key && entry.created.elapsed() <= MAX_AGE && stamp.follows(&entry.stamp)
        })
    }
    pub fn new(state: &Path) -> Self {
        Self {
            entries: VecDeque::new(),
            directory: Directory::new(state).ok(),
            watches: VecDeque::new(),
            hit: false,
        }
    }
    pub fn directory(&self) -> Option<&Path> {
        self.directory.as_ref().map(|d| d.path.as_path())
    }
    pub fn reset_hit(&mut self) {
        self.hit = false;
    }
    pub fn last_was_hit(&self) -> bool {
        self.hit
    }
    pub fn filesystem_stamp(&mut self, root: &Path, excluded: &Path) -> anyhow::Result<Stamp> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let position = self.watches.iter().position(|w| w.root == root);
        let mut watch = if let Some(position) = position {
            self.watches.remove(position).unwrap()
        } else {
            let (tx, rx) = std::sync::mpsc::sync_channel(128);
            let watch = crate::index_watch::Watch::new(
                0,
                root.to_owned(),
                excluded
                    .canonicalize()
                    .unwrap_or_else(|_| excluded.to_owned()),
                tx,
            );
            for _ in 0..10 {
                if watch.ready.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            RootWatch {
                root: root.to_owned(),
                watch,
                rx,
                instance: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as i64,
                generation: 0,
            }
        };
        let changed = watch.rx.try_iter().take(128).count() > 0;
        let lost = watch
            .watch
            .overflow
            .swap(false, std::sync::atomic::Ordering::AcqRel);
        if changed || lost {
            watch.generation += 1;
        }
        let stamp = Stamp {
            filesystem: true,
            identity: identity::capture(root)?.identity.encode().to_vec(),
            revision: watch.instance,
            sequence: watch.generation,
            floor: 0,
            roots: vec![],
            healthy: watch.watch.ready.load(std::sync::atomic::Ordering::Acquire),
            day: chrono::Local::now().date_naive(),
        };
        self.watches.push_back(watch);
        while self.watches.len() > MAX_ENTRIES {
            self.watches.pop_front();
        }
        Ok(stamp)
    }
    pub fn take(&mut self, key: &str, stamp: &Stamp) -> Option<Entry> {
        self.entries.retain(|e| e.created.elapsed() < MAX_AGE);
        let position = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(position)?;
        if !stamp.follows(&entry.stamp) {
            return None;
        }
        // Cache damage is a miss. It must not turn an otherwise valid query into an error.
        let valid = (|| -> rusqlite::Result<()> {
            let db = Connection::open_with_flags(
                &entry.file.0,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            db.prepare("SELECT id,name,path,directory,grp,modified,bytes,source_id,identity FROM results LIMIT 1")?.query([])?.next()?;
            Ok(())
        })();
        valid.ok().map(|_| {
            self.hit = true;
            entry
        })
    }
    pub fn insert(&mut self, entry: Entry) {
        if !entry.stamp.healthy || entry.created.elapsed() >= MAX_AGE {
            return;
        }
        let bytes = |e: &Entry| std::fs::metadata(&e.file.0).map_or(MAX_BYTES + 1, |m| m.len());
        if bytes(&entry) > MAX_BYTES {
            return;
        }
        self.entries
            .retain(|e| e.key != entry.key && e.created.elapsed() < MAX_AGE);
        self.entries.push_back(entry);
        self.trim(MAX_ENTRIES, MAX_BYTES);
    }
    fn trim(&mut self, max_entries: usize, max_bytes: u64) {
        while self.entries.len() > max_entries
            || self
                .entries
                .iter()
                .map(|e| std::fs::metadata(&e.file.0).map_or(MAX_BYTES + 1, |m| m.len()))
                .sum::<u64>()
                > max_bytes
        {
            self.entries.pop_front();
        }
    }
}
struct RootWatch {
    root: PathBuf,
    watch: crate::index_watch::Watch,
    rx: std::sync::mpsc::Receiver<crate::index_watch::Change>,
    instance: i64,
    generation: i64,
}

struct Directory {
    path: PathBuf,
    lock: Option<File>,
}
impl Directory {
    fn new(state: &Path) -> std::io::Result<Self> {
        let base = state.join("search-results-v1");
        std::fs::create_dir_all(&base)?;
        // Never follow links or remove another live window's cache. A stale
        // session is reclaimable only after taking its exclusive ownership file.
        for entry in std::fs::read_dir(&base)?.take(128).flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("session-")
                || !name[8..].chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            {
                continue;
            }
            let Ok(m) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !m.is_dir() || m.file_attributes() & 0x400 != 0 {
                continue;
            }
            if let Ok(lock) = OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(4)
                .open(path.join("owner.lock"))
            {
                let Ok(files) = std::fs::read_dir(&path).and_then(|items| {
                    items
                        .take(65)
                        .map(|e| e.map(|v| v.path()))
                        .collect::<Result<Vec<_>, _>>()
                }) else {
                    continue;
                };
                if files.len() < 65
                    && files.iter().all(|p| {
                        std::fs::symlink_metadata(p)
                            .is_ok_and(|m| m.is_file() && m.file_attributes() & 0x400 == 0)
                            && (p.file_name().is_some_and(|n| n == "owner.lock")
                                || p.extension().is_some_and(|e| e == "sqlite"))
                    })
                {
                    for p in files {
                        let _ = std::fs::remove_file(p);
                    }
                    drop(lock);
                    let _ = std::fs::remove_dir(path);
                }
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = base.join(format!("session-{:x}-{unique:x}", std::process::id()));
        std::fs::create_dir(&path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(path.join("owner.lock"))?;
        Ok(Self {
            path,
            lock: Some(lock),
        })
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        self.lock.take();
        let _ = std::fs::remove_file(self.path.join("owner.lock"));
        let _ = std::fs::remove_dir(&self.path);
    }
}

pub fn candidate(
    db: &Connection,
    id: i64,
    recursive: bool,
    scope: Option<&crate::index::Scope>,
) -> anyhow::Result<Option<crate::index::IndexedItem>> {
    let Some(item) = db
        .query_row(
            "SELECT id,display,path,directory,identity FROM entries WHERE id=?1",
            [id],
            |r| {
                Ok(crate::index::IndexedItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: crate::operations::ledger::decode_path(&r.get::<_, Vec<u8>>(2)?),
                    directory: r.get(3)?,
                    identity: r.get(4)?,
                })
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    if let Some(scope) = scope {
        let folder = crate::operations::ledger::decode_path(&scope.path);
        if !crate::index::inside(&folder, &item.path, scope.recursive) {
            return Ok(None);
        }
        return Ok(Some(item));
    }
    if !recursive {
        let Some(parent) = item.path.parent() else {
            return Ok(None);
        };
        let registered: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM roots WHERE path=?1)",
            [crate::operations::ledger::encode_path(parent)],
            |r| r.get(0),
        )?;
        if !registered {
            return Ok(None);
        }
    }
    Ok(Some(item))
}
pub fn alternate(
    db: &Connection,
    path: &[u8],
    excluded: i64,
    recursive: bool,
    scope: Option<&crate::index::Scope>,
) -> anyhow::Result<Option<crate::index::IndexedItem>> {
    let mut q = db.prepare("SELECT id FROM entries WHERE path=?1 AND id<>?2 LIMIT 64")?;
    let ids = q
        .query_map(rusqlite::params![path, excluded], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(q);
    for id in ids {
        if let Some(item) = candidate(db, id, recursive, scope)? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(path: PathBuf, key: &str, stamp: &Stamp) -> Entry {
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE results(id,name,path,directory,grp,modified,bytes,source_id,identity)",
        )
        .unwrap();
        drop(db);
        Entry {
            key: key.into(),
            stamp: stamp.clone(),
            file: crate::search::Database(path),
            created: Instant::now(),
            skipped: 0,
            count: 0,
        }
    }
    #[test]
    fn lru_budget_expiry_corruption_and_crash_cleanup_preserve_live_sessions() {
        let fixture = crate::test_support::Fixture::new("cache-limits");
        let files = fixture.0.join("files");
        std::fs::create_dir(&files).unwrap();
        let state = fixture.0.join("state");
        let db = crate::index::build_test_index(&files, &state);
        let stamp = Stamp::read(&db, &state).unwrap();
        let mut cache = Cache::new(&state);
        let dir = cache.directory().unwrap().to_path_buf();
        for i in 0..9 {
            cache.insert(entry(
                dir.join(format!("{i}.sqlite")),
                &i.to_string(),
                &stamp,
            ));
        }
        assert_eq!(cache.entries.len(), 8);
        assert!(!dir.join("0.sqlite").exists());
        let used = cache.take("1", &stamp).unwrap();
        cache.insert(used);
        cache.insert(entry(dir.join("9.sqlite"), "9", &stamp));
        assert!(!dir.join("2.sqlite").exists());
        assert!(dir.join("1.sqlite").exists());
        let one = std::fs::metadata(dir.join("1.sqlite")).unwrap().len();
        cache.trim(8, one * 2);
        assert_eq!(cache.entries.len(), 2);
        let mut expired = entry(dir.join("expired.sqlite"), "expired", &stamp);
        expired.created -= MAX_AGE + Duration::from_secs(1);
        cache.insert(expired);
        assert!(!dir.join("expired.sqlite").exists());
        let corrupt = entry(dir.join("bad.sqlite"), "bad", &stamp);
        std::fs::write(&corrupt.file.0, b"broken SQLite").unwrap();
        cache.insert(corrupt);
        assert!(cache.take("bad", &stamp).is_none());
        assert!(!dir.join("bad.sqlite").exists());
        let mut stale = std::mem::ManuallyDrop::new(Directory::new(&state).unwrap());
        let stale_path = stale.path.clone();
        std::fs::write(stale_path.join("result.sqlite"), b"owned stale cache").unwrap();
        drop(stale.lock.take());
        let _other = Cache::new(&state);
        assert!(!stale_path.exists());
        assert!(dir.join("owner.lock").exists());
        assert!(dir.join("1.sqlite").exists());
        drop(cache);
        assert!(!dir.exists());
    }
    #[test]
    fn key_preserves_query_scope_filters_sort_group_and_relative_dates() {
        use crate::file_view::{Group, Layout, Sort};
        let mut view = FileView::new(Path::new(r"C:\owned"));
        view.query = "Report".into();
        view.indexed = true;
        let base = key(&view).unwrap();
        let mut equivalent = view.clone();
        equivalent.query = "report".into();
        assert_eq!(key(&equivalent).unwrap(), base);
        let variants = [
            {
                let mut v = view.clone();
                v.root = crate::file_view::WinPath::new(Path::new(r"D:\owned"));
                v
            },
            {
                let mut v = view.clone();
                v.recursive = false;
                v
            },
            {
                let mut v = view.clone();
                v.indexed = false;
                v
            },
            {
                let mut v = view.clone();
                v.extensions = vec!["txt".into()];
                v
            },
            {
                let mut v = view.clone();
                v.modified_days = Some(7);
                v
            },
            {
                let mut v = view.clone();
                v.min_bytes = Some(1);
                v
            },
            {
                let mut v = view.clone();
                v.max_bytes = Some(100);
                v
            },
            {
                let mut v = view.clone();
                v.sort = Sort::Size;
                v
            },
            {
                let mut v = view.clone();
                v.descending = true;
                v
            },
            {
                let mut v = view.clone();
                v.group = Group::Type;
                v
            },
            {
                let mut v = view.clone();
                v.layout = Layout::Grid;
                v
            },
        ];
        for v in variants {
            assert_ne!(key(&v).unwrap(), base);
        }
        let mut next_day = Stamp {
            filesystem: false,
            identity: vec![],
            revision: 1,
            sequence: 1,
            floor: 1,
            roots: vec![],
            healthy: true,
            day: chrono::Local::now().date_naive(),
        };
        let previous = next_day.clone();
        next_day.day = next_day.day.succ_opt().unwrap();
        assert!(!next_day.follows(&previous));
    }
}
