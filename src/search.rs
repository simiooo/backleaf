//! Local, read-only filename search. One latest-only worker; directory frontier and
//! results spill to SQLite instead of retaining a tree or result set in UI memory.
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    ops::Range,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Row {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub parent: String,
    pub directory: bool,
    pub group_heading: Option<String>,
}
pub enum Notice {
    Invalidated {
        token: u64,
    },
    Progress {
        token: u64,
        count: usize,
        scanned: usize,
        unavailable: usize,
        skipped: usize,
        done: bool,
        error: Option<String>,
    },
    Page {
        token: u64,
        serial: u64,
        rows: Vec<Row>,
    },
}
struct Request {
    token: u64,
    root: PathBuf,
    definition: crate::file_view::FileView,
}
struct Shared {
    request: Mutex<Option<Request>>,
    page: Mutex<Option<(u64, u64, Range<usize>)>>,
    changed: Condvar,
    stop: Arc<AtomicBool>,
    latest: Arc<AtomicU64>,
    state_dir: PathBuf,
}
pub struct Worker(Arc<Shared>);
impl Drop for Worker {
    fn drop(&mut self) {
        let _pending = self.0.request.lock().unwrap();
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_one();
    }
}
impl Worker {
    #[cfg(test)]
    pub fn new(tx: async_channel::Sender<crate::model::Event>) -> Self {
        Self::with_state_dir(
            tx,
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("Backleaf"),
        )
    }
    pub fn with_state_dir(
        tx: async_channel::Sender<crate::model::Event>,
        state_dir: PathBuf,
    ) -> Self {
        let shared = Arc::new(Shared {
            request: Mutex::new(None),
            page: Mutex::new(None),
            changed: Condvar::new(),
            stop: Arc::new(AtomicBool::new(false)),
            latest: Arc::new(AtomicU64::new(0)),
            state_dir,
        });
        let state = shared.clone();
        std::thread::Builder::new()
            .name("backleaf-search".into())
            .spawn(move || {
                let mut cache = crate::search_cache::Cache::new(&state.state_dir);
                loop {
                    let request = {
                        let mut pending = state.request.lock().unwrap();
                        while pending.is_none() && !state.stop.load(Ordering::Acquire) {
                            pending = state.changed.wait(pending).unwrap();
                        }
                        if state.stop.load(Ordering::Acquire) {
                            return;
                        }
                        pending.take().unwrap()
                    };
                    let token = request.token;
                    cache.reset_hit();
                    let mut result = scan(&state, &request, &tx, &mut cache);
                    if result.is_err() && current(&state, token) && cache.last_was_hit() {
                        cache.reset_hit();
                        result = scan(&state, &request, &tx, &mut cache);
                    }
                    if let Err(error) = result
                        && current(&state, token)
                    {
                        let _ = tx.send_blocking(crate::model::Event::Search(Notice::Progress {
                            token,
                            count: 0,
                            scanned: 0,
                            unavailable: 0,
                            skipped: 0,
                            done: true,
                            error: Some(format!("无法搜索此位置：{error}")),
                        }));
                    }
                }
            })
            .expect("search worker");
        Self(shared)
    }
    #[cfg(test)]
    pub fn search(&self, token: u64, root: PathBuf, query: String) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.page.lock().unwrap() = None;
        *self.0.request.lock().unwrap() = Some(Request {
            token,
            definition: {
                let mut d = crate::file_view::FileView::new(&root);
                d.query = query.clone();
                d
            },
            root,
        });
        self.0.changed.notify_one();
    }
    pub fn search_view(&self, token: u64, definition: crate::file_view::FileView) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.page.lock().unwrap() = None;
        *self.0.request.lock().unwrap() = Some(Request {
            token,
            root: definition.root.path(),
            definition,
        });
        self.0.changed.notify_one();
    }
    pub fn cancel(&self, token: u64) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.request.lock().unwrap() = None;
        self.0.changed.notify_one();
    }
    pub fn page(&self, token: u64, serial: u64, range: Range<usize>) {
        let _pending = self.0.request.lock().unwrap();
        *self.0.page.lock().unwrap() = Some((
            token,
            serial,
            range.start..range.end.min(range.start.saturating_add(32)),
        ));
        self.0.changed.notify_one();
    }
}
fn current(s: &Shared, token: u64) -> bool {
    !s.stop.load(Ordering::Acquire) && s.latest.load(Ordering::Acquire) == token
}
pub(crate) struct Database(pub(crate) PathBuf);
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn encode(path: &std::path::Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn decode(raw: &[u8]) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(
        &raw.as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .into()
}
#[cfg(test)]
fn matches(name: &str, query: &str) -> bool {
    name.to_lowercase().contains(query)
}
fn insert_candidate(
    db: &Connection,
    candidate: &crate::index::IndexedItem,
    definition: &crate::file_view::FileView,
    now: std::time::SystemTime,
    id: Option<usize>,
    unavailable: &mut usize,
) -> anyhow::Result<bool> {
    let metadata = match crate::operations::windows::identity::capture(&candidate.path) {
        Ok(m) if m.identity.encode().as_slice() == candidate.identity.as_slice() => m,
        _ => {
            *unavailable += 1;
            return Ok(false);
        }
    };
    let file_metadata = definition
        .metadata_needed()
        .then(|| std::fs::metadata(&candidate.path).ok())
        .flatten();
    if !definition.accepts(
        &candidate.path,
        &candidate.name,
        candidate.directory,
        file_metadata.as_ref(),
        now,
    ) {
        return Ok(false);
    }
    Ok(db.execute("INSERT OR IGNORE INTO results VALUES(coalesce(?1,(SELECT coalesce(max(id),-1)+1 FROM results)),?2,?3,?4,?5,?6,?7,?8,?9)",params![id.map(|n|n as i64),candidate.name,encode(&candidate.path),candidate.directory,definition.group_key(&candidate.path,candidate.directory,Some(metadata.modified)),metadata.modified as i64,metadata.bytes.map(|n|n as i64),candidate.id,candidate.identity])?!=0)
}
fn scan(
    s: &Shared,
    r: &Request,
    tx: &async_channel::Sender<crate::model::Event>,
    cache: &mut crate::search_cache::Cache,
) -> anyhow::Result<()> {
    r.definition.validate().map_err(anyhow::Error::msg)?;
    let root = if r.definition.indexed {
        r.root.clone()
    } else {
        r.root.canonicalize()?
    };
    let mut indexed = if r.definition.indexed {
        Some(crate::index::open(&s.state_dir)?)
    } else {
        None
    };
    let stamp = Some(if let Some(index) = &indexed {
        crate::search_cache::Stamp::read(index, &s.state_dir)?
    } else {
        cache.filesystem_stamp(&root, &s.state_dir)?
    });
    let key = crate::search_cache::key(&r.definition)?;
    let cached = stamp
        .as_ref()
        .and_then(|stamp| cache.take(&key, stamp))
        .and_then(|entry| {
            let ids = if let Some(index) = &indexed {
                stamp.as_ref().unwrap().changed_ids(index, &entry.stamp)
            } else {
                Ok(Vec::new())
            };
            ids.ok().map(|ids| (entry, ids))
        });
    let reused = cached.is_some();
    let created = cached
        .as_ref()
        .map_or_else(Instant::now, |(entry, _)| entry.created);
    let mut count = cached.as_ref().map_or(0, |(entry, _)| entry.count);
    let mut skipped = cached.as_ref().map_or(0, |(entry, _)| entry.skipped);
    let temp = cache
        .directory()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("Backleaf"));
    std::fs::create_dir_all(&temp)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let (file, delta) = cached.map_or_else(
        || {
            (
                Database(temp.join(format!(
                    "search-{}-{}-{unique}.sqlite",
                    std::process::id(),
                    r.token
                ))),
                Vec::new(),
            )
        },
        |(entry, ids)| (entry.file, ids),
    );
    let mut db = Connection::open(&file.0)?;
    let latest = s.latest.clone();
    let stop = s.stop.clone();
    let token = r.token;
    db.progress_handler(
        2048,
        Some(move || stop.load(Ordering::Acquire) || latest.load(Ordering::Acquire) != token),
    )?;
    if !reused {
        db.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-512; CREATE TABLE frontier(id INTEGER PRIMARY KEY,path BLOB NOT NULL); CREATE TABLE results(id INTEGER PRIMARY KEY,name TEXT NOT NULL,path BLOB UNIQUE NOT NULL,directory INTEGER NOT NULL,grp TEXT NOT NULL,modified INTEGER,bytes INTEGER,source_id INTEGER,identity BLOB); PRAGMA temp_store=FILE;")?;
    } else {
        db.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-512; PRAGMA temp_store=FILE;")?;
    }
    if let Some(index) = &mut indexed {
        let latest = s.latest.clone();
        let stop = s.stop.clone();
        let token = r.token;
        index.progress_handler(
            2048,
            Some(move || stop.load(Ordering::Acquire) || latest.load(Ordering::Acquire) != token),
        )?;
    }
    let mut index_cursor = 0i64;
    let mut entries = if indexed.is_none() && !reused {
        Some(std::fs::read_dir(&root)?)
    } else {
        None
    };
    let mut scanned = 0usize;
    let mut unavailable = 0usize;
    let mut done = reused;
    let mut last = Instant::now() - Duration::from_secs(1);
    r.definition.validate().map_err(anyhow::Error::msg)?;
    let now = std::time::SystemTime::now();
    let mut sorted = reused && delta.is_empty();
    let mut completion_checked = false;
    let mut reusable_stamp = None;
    let mut announced_done = false;
    let mut invalidation_sent = false;
    let excluded_cache = s
        .state_dir
        .canonicalize()
        .unwrap_or_else(|_| s.state_dir.clone())
        .join("search-results-v1");
    let excluded_temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Backleaf");
    if reused && let Some(index) = &indexed {
        let batch = db.transaction()?;
        for id in &delta {
            if !current(s, r.token) {
                return Ok(());
            }
            scanned += 1;
            let previous_path = batch
                .query_row("SELECT path FROM results WHERE source_id=?1", [id], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .optional()?;
            batch.execute("DELETE FROM results WHERE source_id=?1", [id])?;
            if let Some(candidate) =
                crate::search_cache::candidate(index, *id, r.definition.recursive)?
            {
                insert_candidate(
                    &batch,
                    &candidate,
                    &r.definition,
                    now,
                    None,
                    &mut unavailable,
                )?;
            }
            // Overlapping registered roots can expose the same path through a
            // second indexed entry. Removing its representative must not hide it.
            if let Some(path) = previous_path {
                let present: bool = batch.query_row(
                    "SELECT EXISTS(SELECT 1 FROM results WHERE path=?1)",
                    [&path],
                    |r| r.get(0),
                )?;
                if !present
                    && let Some(candidate) =
                        crate::search_cache::alternate(index, &path, *id, r.definition.recursive)?
                {
                    insert_candidate(
                        &batch,
                        &candidate,
                        &r.definition,
                        now,
                        None,
                        &mut unavailable,
                    )?;
                }
            }
        }
        batch.commit()?;
        count = db.query_row("SELECT count(*) FROM results", [], |row| {
            row.get::<_, i64>(0)
        })? as usize;
    }
    while current(s, r.token) && !tx.is_closed() {
        if done
            && !invalidation_sent
            && r.definition.modified_days.is_some()
            && stamp.as_ref().is_some_and(|start| !start.date_is_current())
        {
            reusable_stamp = None;
            invalidation_sent = true;
            tx.send_blocking(crate::model::Event::Search(Notice::Invalidated {
                token: r.token,
            }))?;
        }
        if done
            && indexed.is_none()
            && !invalidation_sent
            && let Some(start) = &stamp
            && start.healthy
        {
            let end = cache.filesystem_stamp(&root, &s.state_dir)?;
            if &end != start {
                reusable_stamp = None;
                invalidation_sent = true;
                tx.send_blocking(crate::model::Event::Search(Notice::Invalidated {
                    token: r.token,
                }))?;
            }
        }
        if !done {
            let batch = db.transaction()?;
            if let Some(index) = &indexed {
                let candidates = crate::index::query_batch(
                    index,
                    &r.definition.query,
                    index_cursor,
                    r.definition.recursive,
                )?;
                done = candidates.is_empty();
                for candidate in candidates {
                    if !current(s, r.token) {
                        return Ok(());
                    }
                    index_cursor = candidate.id;
                    scanned += 1;
                    count += usize::from(insert_candidate(
                        &batch,
                        &candidate,
                        &r.definition,
                        now,
                        Some(count),
                        &mut unavailable,
                    )?);
                }
            } else {
                for _ in 0..256 {
                    if !current(s, r.token) {
                        return Ok(());
                    }
                    let next = entries.as_mut().and_then(Iterator::next);
                    match next {
                        Some(Ok(entry)) => {
                            let path = entry.path();
                            if path.starts_with(&excluded_cache) || path.starts_with(&excluded_temp)
                            {
                                continue;
                            }
                            scanned += 1;
                            let name = entry.file_name().to_string_lossy().into_owned();
                            let kind = entry.file_type();
                            let metadata = r
                                .definition
                                .metadata_needed()
                                .then(|| entry.metadata().ok())
                                .flatten();
                            let directory = kind.as_ref().is_ok_and(|k| k.is_dir());
                            if r.definition
                                .accepts(&path, &name, directory, metadata.as_ref(), now)
                            {
                                batch.execute(
                                    "INSERT INTO results VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,NULL)",
                                    params![
                                        count as i64,
                                        name,
                                        encode(&path),
                                        directory,
                                        r.definition.group_key(
                                            &path,
                                            directory,
                                            metadata
                                                .as_ref()
                                                .and_then(|m| m.modified().ok())
                                                .and_then(|t| t
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .ok())
                                                .map(|t| t.as_secs())
                                        ),
                                        metadata
                                            .as_ref()
                                            .and_then(|m| m.modified().ok())
                                            .and_then(|t| t
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .ok())
                                            .map(|t| t.as_secs() as i64),
                                        metadata
                                            .as_ref()
                                            .filter(|m| !m.is_dir())
                                            .map(|m| m.len() as i64)
                                    ],
                                )?;
                                count += 1;
                            }
                            match kind {
                                Ok(kind) if kind.is_dir() && r.definition.recursive => {
                                    use std::os::windows::fs::MetadataExt;
                                    match entry.metadata() {
                                        Ok(metadata) if metadata.file_attributes() & 0x400 == 0 => {
                                            batch.execute(
                                                "INSERT INTO frontier(path) VALUES(?1)",
                                                [encode(&path)],
                                            )?;
                                        }
                                        Ok(_) => skipped += 1,
                                        Err(_) => unavailable += 1,
                                    }
                                }
                                Ok(kind) if kind.is_symlink() => skipped += 1,
                                Err(_) => unavailable += 1,
                                _ => {}
                            }
                        }
                        Some(Err(_)) => {
                            scanned += 1;
                            unavailable += 1;
                        }
                        None => {
                            let next = batch
                                .query_row(
                                    "SELECT id,path FROM frontier ORDER BY id LIMIT 1",
                                    [],
                                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                                )
                                .optional()?;
                            if let Some((id, path)) = next {
                                batch.execute("DELETE FROM frontier WHERE id=?1", [id])?;
                                match std::fs::read_dir(decode(&path)) {
                                    Ok(dir) => entries = Some(dir),
                                    Err(_) => {
                                        entries = None;
                                        unavailable += 1;
                                    }
                                }
                            } else {
                                done = true;
                                break;
                            }
                        }
                    }
                }
            }
            batch.commit()?;
        }
        if done && !sorted {
            let sort = match r.definition.sort {
                crate::file_view::Sort::Name => "name COLLATE NOCASE",
                crate::file_view::Sort::Modified => "modified",
                crate::file_view::Sort::Size => "bytes",
            };
            let direction = if r.definition.descending {
                "DESC"
            } else {
                "ASC"
            };
            let order = format!("grp,directory DESC,{sort} {direction},path");
            db.execute_batch(&format!("CREATE TABLE ordered AS SELECT row_number() OVER(ORDER BY {order})-1 AS id,name,path,directory,grp,modified,bytes,source_id,identity FROM results; DROP TABLE results; ALTER TABLE ordered RENAME TO results; CREATE UNIQUE INDEX search_page ON results(id); CREATE UNIQUE INDEX cached_source ON results(source_id); CREATE UNIQUE INDEX cached_path ON results(path);"))?;
            sorted = true;
        }
        if done && !completion_checked {
            completion_checked = true;
            if unavailable == 0
                && let Some(start) = &stamp
            {
                let end = if let Some(index) = &indexed {
                    crate::search_cache::Stamp::read(index, &s.state_dir)?
                } else {
                    cache.filesystem_stamp(&root, &s.state_dir)?
                };
                if &end == start && end.healthy {
                    reusable_stamp = Some(end);
                }
            }
        }
        let page = s.page.lock().unwrap().take();
        if let Some((token, serial, range)) = page
            && token == r.token
        {
            let mut statement = db.prepare_cached(
                "SELECT id,name,path,directory,CASE WHEN grp<>coalesce((SELECT grp FROM results prior WHERE prior.id=results.id-1),'') THEN grp END,identity,modified,bytes FROM results WHERE id>=?1 AND id<?2 ORDER BY id LIMIT 32",
            )?;
            let mut rows = Vec::new();
            let mut invalidated = false;
            for row in
                statement.query_map(params![range.start as i64, range.end as i64], |row| {
                    let path = decode(&row.get::<_, Vec<u8>>(2)?);
                    let parent = path
                        .parent()
                        .and_then(|p| p.strip_prefix(&root).ok())
                        .map(|p| p.to_string_lossy().into_owned())
                        .filter(|p| !p.is_empty())
                        .unwrap_or_else(|| "此文件夹".into());
                    Ok((
                        Row {
                            id: row.get::<_, i64>(0)? as usize,
                            name: row.get(1)?,
                            path,
                            parent,
                            directory: row.get(3)?,
                            group_heading: row.get(4)?,
                        },
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                    ))
                })?
            {
                let (row, expected, modified, bytes) = row?;
                if let Some(expected) = expected {
                    let valid =
                        crate::operations::windows::identity::capture(&row.path).is_ok_and(|m| {
                            m.identity.encode().as_slice() == expected.as_slice()
                                && Some(m.modified as i64) == modified
                                && m.bytes.map(|n| n as i64) == bytes
                        });
                    if !valid {
                        invalidated = true;
                        continue;
                    }
                } else {
                    let valid = std::fs::symlink_metadata(&row.path).is_ok_and(|m| {
                        m.is_dir() == row.directory
                            && r.definition.accepts(
                                &row.path,
                                &row.name,
                                row.directory,
                                Some(&m),
                                now,
                            )
                            && (!r.definition.metadata_needed()
                                || (m
                                    .modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|t| t.as_secs() as i64)
                                    == modified
                                    && (!m.is_dir()).then_some(m.len() as i64) == bytes))
                    });
                    if !valid {
                        invalidated = true;
                        continue;
                    }
                }
                rows.push(row);
            }
            if invalidated && current(s, r.token) {
                reusable_stamp = None;
                tx.send_blocking(crate::model::Event::Search(Notice::Invalidated {
                    token: r.token,
                }))?;
                continue;
            }
            if current(s, r.token) {
                tx.send_blocking(crate::model::Event::Search(Notice::Page {
                    token: r.token,
                    serial,
                    rows,
                }))?;
            }
        }
        if (!done && last.elapsed() >= Duration::from_millis(100)) || (done && !announced_done) {
            tx.send_blocking(crate::model::Event::Search(Notice::Progress {
                token: r.token,
                count,
                scanned,
                unavailable,
                skipped,
                done,
                error: None,
            }))?;
            last = Instant::now();
            announced_done = done;
        }
        if done {
            // Keep the completed result DB available for paged scrolling; do not emit idle notices.
            let mut pending = s.request.lock().unwrap();
            while pending.is_none() && s.page.lock().unwrap().is_none() && current(s, r.token) {
                if indexed.is_none() {
                    let (next, timeout) = s
                        .changed
                        .wait_timeout(pending, Duration::from_millis(250))
                        .unwrap();
                    pending = next;
                    if timeout.timed_out() {
                        break;
                    }
                } else {
                    let (next, timeout) = s
                        .changed
                        .wait_timeout(pending, Duration::from_secs(1))
                        .unwrap();
                    pending = next;
                    if timeout.timed_out() {
                        break;
                    }
                }
            }
        }
    }
    drop(db);
    if let Some(stamp) = reusable_stamp {
        cache.insert(crate::search_cache::Entry {
            key,
            stamp,
            file,
            created,
            skipped,
            count,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Notice, Worker, matches};
    use crate::model::Event;
    struct IndexedFixture {
        root: PathBuf,
        state: PathBuf,
        index: rusqlite::Connection,
        worker: Worker,
        rx: async_channel::Receiver<Event>,
        token: u64,
        _fixture: crate::test_support::Fixture,
    }
    use std::path::PathBuf;
    impl IndexedFixture {
        fn new() -> Self {
            let fixture = crate::test_support::Fixture::new("result-cache");
            let root = fixture.0.join("files");
            let state = fixture.0.join("state");
            std::fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            for name in ["报告_A.txt", "报告_B.txt", "报告_C.pdf", "其他.txt"] {
                std::fs::write(root.join(name), b"owned").unwrap();
            }
            let index = crate::index::build_test_index(&root, &state);
            let (tx, rx) = async_channel::unbounded();
            let worker = Worker::with_state_dir(tx, state.clone());
            Self {
                _fixture: fixture,
                root,
                state,
                index,
                worker,
                rx,
                token: 0,
            }
        }
        fn view(&self, query: &str) -> crate::file_view::FileView {
            let mut view = crate::file_view::FileView::new(&self.root);
            view.indexed = true;
            view.query = query.into();
            view
        }
        fn changed(&mut self, path: &std::path::Path) {
            let root = self
                .index
                .query_row("SELECT id FROM roots", [], |r| r.get(0))
                .unwrap();
            crate::index::incremental(&mut self.index, root, path, &self.state).unwrap();
        }
        fn query(&mut self, view: crate::file_view::FileView) -> (usize, usize, Vec<super::Row>) {
            self.token += 1;
            let mut token = self.token;
            self.worker.search_view(token, view.clone());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let mut result = None;
            while std::time::Instant::now() < deadline {
                match self.rx.try_recv() {
                    Ok(Event::Search(Notice::Progress {
                        token: t,
                        count,
                        scanned,
                        done: true,
                        error,
                        ..
                    })) if t == token => {
                        assert!(error.is_none(), "{error:?}");
                        result = Some((count, scanned));
                        self.worker.page(token, token, 0..32);
                    }
                    Ok(Event::Search(Notice::Page { token: t, rows, .. })) if t == token => {
                        let (count, scanned) = result.unwrap();
                        return (count, scanned, rows);
                    }
                    Ok(Event::Search(Notice::Invalidated { token: t })) if t == token => {
                        self.token += 1;
                        token = self.token;
                        result = None;
                        self.worker.search_view(token, view.clone());
                    }
                    _ => std::thread::sleep(std::time::Duration::from_millis(1)),
                }
            }
            panic!("Search timed out");
        }
    }
    #[test]
    fn indexed_cache_reuses_completed_queries_and_patches_created_renamed_deleted_and_filtered_files()
     {
        let mut f = IndexedFixture::new();
        let report = f.view("报");
        let other = f.view("其他");
        let (count, scanned, _) = f.query(report.clone());
        assert_eq!((count, scanned), (3, 3));
        f.query(other.clone());
        assert_eq!(f.query(report.clone()).1, 0);
        let new = f.root.join("报告_D.txt");
        std::fs::write(&new, b"owned").unwrap();
        f.changed(&new);
        let (count, scanned, _) = f.query(report.clone());
        assert_eq!((count, scanned), (4, 1));
        let renamed = f.root.join("其他_D.txt");
        std::fs::rename(&new, &renamed).unwrap();
        f.changed(&new);
        f.changed(&renamed);
        let (count, scanned, rows) = f.query(report.clone());
        assert_eq!(count, 3);
        assert!((1..=2).contains(&scanned));
        assert!(!rows.iter().any(|r| r.path == new));
        let removed = f.root.join("报告_B.txt");
        std::fs::remove_file(&removed).unwrap();
        f.changed(&removed);
        assert_eq!(f.query(report.clone()).0, 2);
        let mut filtered = report.clone();
        filtered.max_bytes = Some(10);
        assert_eq!(f.query(filtered.clone()).0, 2);
        let grown = f.root.join("报告_A.txt");
        std::fs::write(&grown, vec![b'x'; 64]).unwrap();
        f.changed(&grown);
        let (count, scanned, rows) = f.query(filtered);
        assert_eq!((count, scanned), (1, 1));
        assert_eq!(rows[0].name, "报告_C.pdf");
        let negative = f.view("新增");
        assert_eq!(f.query(negative.clone()).0, 0);
        let new = f.root.join("新增.txt");
        std::fs::write(&new, b"new").unwrap();
        f.changed(&new);
        assert_eq!(f.query(negative).0, 1);
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn indexed_cache_missed_journal_and_rebuild_fall_back_to_full_queries() {
        let mut f = IndexedFixture::new();
        let view = f.view("报");
        assert_eq!(f.query(view.clone()).1, 3);
        let id: i64 = f
            .index
            .query_row(
                "SELECT id FROM entries WHERE display='报告_A.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let txn = f.index.transaction().unwrap();
        for _ in 0..8200 {
            txn.execute("INSERT INTO search_changes(entry)VALUES(?1)", [id])
                .unwrap();
        }
        txn.commit().unwrap();
        assert_eq!(
            f.index
                .query_row("SELECT count(*) FROM search_changes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            8192
        );
        assert_eq!(f.query(view.clone()).1, 3);
        f.index.execute("UPDATE roots SET building=1", []).unwrap();
        assert_eq!(f.query(view.clone()).1, 3);
        f.index.execute("UPDATE roots SET building=0", []).unwrap();
        assert_eq!(f.query(view.clone()).1, 3);
        assert_eq!(f.query(view).1, 0);
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn indexed_cached_visible_page_rejects_replaced_identity_before_watcher_update() {
        let mut f = IndexedFixture::new();
        let view = f.view("报");
        f.query(view.clone());
        let old = f.root.join("报告_A.txt");
        let kept = f.root.join("kept.txt");
        std::fs::rename(&old, &kept).unwrap();
        std::fs::write(&old, b"replacement").unwrap();
        f.worker.page(f.token, 99, 0..32);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut invalidated = false;
        while std::time::Instant::now() < deadline {
            match f.rx.try_recv() {
                Ok(Event::Search(Notice::Invalidated { token })) if token == f.token => {
                    invalidated = true;
                    break;
                }
                Ok(Event::Search(Notice::Page { serial: 99, .. })) => {
                    panic!("Replaced identity was displayed")
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        }
        assert!(invalidated);
        f.changed(&old);
        f.changed(&kept);
        let (count, scanned, _) = f.query(view);
        assert_eq!((count, scanned), (3, 3));
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn directory_query_cache_watches_recursive_changes_and_invalidates_without_an_index() {
        let mut f = IndexedFixture::new();
        let sub = f.root.join("nested");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("报告_deep.txt"), b"owned").unwrap();
        let mut view = f.view("报");
        view.indexed = false;
        assert_eq!(f.query(view.clone()).0, 4);
        let mut warm = f.query(view.clone());
        if warm.1 != 0 {
            warm = f.query(view.clone());
        }
        assert_eq!(warm.1, 0);
        let new = sub.join("报告_new.txt");
        std::fs::write(&new, b"new").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut invalidated = false;
        while std::time::Instant::now() < deadline {
            if matches!(f.rx.try_recv(),Ok(Event::Search(Notice::Invalidated{token})) if token==f.token)
            {
                invalidated = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            invalidated,
            "Recursive watch did not invalidate the active query"
        );
        let (count, scanned, rows) = f.query(view.clone());
        assert_eq!(count, 5);
        assert!(scanned > 0);
        assert!(rows.iter().any(|r| r.path == new));
        assert_eq!(f.query(view).1, 0);
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn overlapping_index_cache_roots_keep_one_result_when_a_representative_entry_disappears() {
        let mut f = IndexedFixture::new();
        drop(crate::index::build_test_index(&f._fixture.0, &f.state));
        let view = f.view("报");
        assert_eq!(f.query(view.clone()).0, 3);
        assert_eq!(f.query(view.clone()).1, 0);
        let path = super::encode(&f.root.join("报告_A.txt"));
        f.index
            .execute(
                "DELETE FROM entries WHERE id=(SELECT min(id) FROM entries WHERE path=?1)",
                [&path],
            )
            .unwrap();
        let (count, _, rows) = f.query(view);
        assert_eq!(count, 3);
        assert_eq!(rows.iter().filter(|r| r.name == "报告_A.txt").count(), 1);
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn indexed_cache_case_only_rename_uses_actual_spelling_without_duplicate_paths() {
        let mut f = IndexedFixture::new();
        let view = f.view("报");
        f.query(view.clone());
        let old = f.root.join("报告_A.txt");
        let new = f.root.join("报告_a.TXT");
        std::fs::rename(&old, &new).unwrap();
        f.changed(&old);
        f.changed(&new);
        let (count, _, rows) = f.query(view);
        assert_eq!(count, 3);
        assert!(rows.iter().any(|r| r.name == "报告_a.TXT"));
        assert!(!rows.iter().any(|r| r.name == "报告_A.txt"));
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    #[ignore = "Explicit real-file cache performance measurement"]
    fn indexed_search_cache_real_files_benchmark() {
        let mut f = IndexedFixture::new();
        for i in 0..10000 {
            let p = f.root.join(format!("报告_{i:05}.txt"));
            std::fs::write(&p, b"owned cache benchmark").unwrap();
            f.changed(&p);
        }
        let mut cold = Vec::new();
        let mut warm = Vec::new();
        let mut delta = Vec::new();
        for i in 0..5 {
            let mut view = f.view("报");
            view.min_bytes = Some(i);
            let start = std::time::Instant::now();
            let (count, scanned, _) = f.query(view.clone());
            cold.push(start.elapsed().as_secs_f64() * 1000.);
            assert_eq!(count, 10003);
            assert_eq!(scanned, 10003);
            let start = std::time::Instant::now();
            assert_eq!(f.query(view.clone()).1, 0);
            warm.push(start.elapsed().as_secs_f64() * 1000.);
            let path = f.root.join("报告_00000.txt");
            std::fs::write(&path, vec![b'x'; 100 + i as usize]).unwrap();
            f.changed(&path);
            let start = std::time::Instant::now();
            assert_eq!(f.query(view).1, 1);
            delta.push(start.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "SEARCH_CACHE_BENCH {}",
            serde_json::json!({"real_files":10004,"matching_files":10003,"runs":5,"cold_ms":cold,"warm_ms":warm,"incremental_one_file_ms":delta,"warm_candidate_checks":0,"incremental_candidate_checks":1,"page_identity_checks_max":32,"scope":"Actual owned files, persistent SQLite index, production search worker and first bounded page; warm OS disk cache, no UI/GPU or input-to-photon claim"})
        );
        drop(f.worker);
        drop(f.rx);
    }
    #[test]
    fn literal_unicode_matching_does_not_treat_wildcards_as_operators() {
        assert!(matches("季度报告.PDF", "报"));
        assert!(matches("Résumé.PDF", "résumé"));
        assert!(!matches("report.pdf", "%"));
        assert!(matches("A_[1].txt", "_[1]"));
    }
    #[test]
    fn recursive_results_page_and_latest_request_are_real_and_bounded() {
        let root = std::env::temp_dir().join(format!(
            "backleaf-search-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("子目录")).unwrap();
        for i in 0..140 {
            std::fs::write(
                root.join("子目录").join(format!("报告_{i}.PDF")),
                b"owned fixture",
            )
            .unwrap();
        }
        std::fs::write(root.join("unmatched.txt"), b"keep").unwrap();
        std::fs::create_dir(root.join("报告_目录.PDF")).unwrap();
        let (tx, rx) = async_channel::bounded(8);
        let worker = Worker::new(tx);
        worker.search(1, root.clone(), "unmatched".into());
        worker.search(2, root.clone(), "报".into());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut done = false;
        let mut page = false;
        while std::time::Instant::now() < deadline {
            if let Ok(event) = rx.try_recv() {
                match event {
                    Event::Search(Notice::Progress {
                        token: 2,
                        count,
                        done: true,
                        unavailable,
                        ..
                    }) => {
                        assert_eq!(count, 141);
                        assert_eq!(unavailable, 0);
                        done = true;
                        worker.page(2, 3, 90..140);
                    }
                    Event::Search(Notice::Page {
                        token: 2,
                        serial: 3,
                        rows,
                    }) => {
                        assert_eq!(rows.len(), 32);
                        assert_eq!(rows[0].id, 90);
                        assert!(rows.iter().all(|r| r.path.exists()
                            && r.name.starts_with("报告")
                            && !r.directory));
                        worker.page(2, 4, 0..32);
                    }
                    Event::Search(Notice::Page {
                        token: 2,
                        serial: 4,
                        rows,
                    }) => {
                        assert_eq!(rows.len(), 32);
                        assert_eq!(rows[0].name, "报告_目录.PDF");
                        assert!(rows[0].directory);
                        page = true;
                        break;
                    }
                    _ => {}
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert!(done && page);
        drop(worker);
        drop(rx);
        assert!(
            root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("backleaf-search-test-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
