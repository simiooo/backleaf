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
                    if let Err(error) = scan(&state, &request, &tx)
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
struct Database(PathBuf);
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
fn scan(
    s: &Shared,
    r: &Request,
    tx: &async_channel::Sender<crate::model::Event>,
) -> anyhow::Result<()> {
    let cache = std::env::temp_dir().join("Backleaf");
    std::fs::create_dir_all(&cache)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let file = Database(cache.join(format!(
        "search-{}-{}-{unique}.sqlite",
        std::process::id(),
        r.token
    )));
    let mut db = Connection::open(&file.0)?;
    let latest = s.latest.clone();
    let stop = s.stop.clone();
    let token = r.token;
    db.progress_handler(
        2048,
        Some(move || stop.load(Ordering::Acquire) || latest.load(Ordering::Acquire) != token),
    )?;
    db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-512; CREATE TABLE frontier(id INTEGER PRIMARY KEY,path BLOB NOT NULL); CREATE TABLE results(id INTEGER PRIMARY KEY,name TEXT NOT NULL,path BLOB UNIQUE NOT NULL,directory INTEGER NOT NULL,grp TEXT NOT NULL,modified INTEGER,bytes INTEGER); PRAGMA temp_store=FILE;")?;
    // Root may itself be a Known Folder redirection; never recurse through child reparse points.
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
    let mut entries = if indexed.is_none() {
        Some(std::fs::read_dir(&root)?)
    } else {
        None
    };
    let mut count = 0usize;
    let mut scanned = 0usize;
    let mut unavailable = 0usize;
    let mut skipped = 0usize;
    let mut done = false;
    let mut last = Instant::now() - Duration::from_secs(1);
    r.definition.validate().map_err(anyhow::Error::msg)?;
    let now = std::time::SystemTime::now();
    let mut sorted = false;
    while current(s, r.token) && !tx.is_closed() {
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
                    let metadata =
                        match crate::operations::windows::identity::capture(&candidate.path) {
                            Ok(m)
                                if m.identity.encode().as_slice()
                                    == candidate.identity.as_slice() =>
                            {
                                m
                            }
                            _ => {
                                unavailable += 1;
                                continue;
                            }
                        };
                    let file_metadata = r
                        .definition
                        .metadata_needed()
                        .then(|| std::fs::metadata(&candidate.path).ok())
                        .flatten();
                    if r.definition.accepts(
                        &candidate.path,
                        &candidate.name,
                        candidate.directory,
                        file_metadata.as_ref(),
                        now,
                    ) {
                        let inserted = batch.execute(
                            "INSERT OR IGNORE INTO results VALUES(?1,?2,?3,?4,?5,?6,?7)",
                            params![
                                count as i64,
                                candidate.name,
                                encode(&candidate.path),
                                candidate.directory,
                                r.definition.group_key(
                                    &candidate.path,
                                    candidate.directory,
                                    Some(metadata.modified)
                                ),
                                metadata.modified as i64,
                                metadata.bytes.map(|n| n as i64)
                            ],
                        )?;
                        count += inserted;
                    }
                }
            } else {
                for _ in 0..256 {
                    if !current(s, r.token) {
                        return Ok(());
                    }
                    let next = entries.as_mut().and_then(Iterator::next);
                    match next {
                        Some(Ok(entry)) => {
                            scanned += 1;
                            let name = entry.file_name().to_string_lossy().into_owned();
                            let path = entry.path();
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
                                    "INSERT INTO results VALUES(?1,?2,?3,?4,?5,?6,?7)",
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
            db.execute_batch(&format!("CREATE TABLE ordered AS SELECT row_number() OVER(ORDER BY {order})-1 AS id,name,path,directory,grp,modified,bytes FROM results; DROP TABLE results; ALTER TABLE ordered RENAME TO results; CREATE UNIQUE INDEX search_page ON results(id);"))?;
            sorted = true;
        }
        let page = s.page.lock().unwrap().take();
        if let Some((token, serial, range)) = page
            && token == r.token
        {
            let mut statement = db.prepare_cached(
                "SELECT id,name,path,directory,CASE WHEN grp<>coalesce((SELECT grp FROM results prior WHERE prior.id=results.id-1),'') THEN grp END FROM results WHERE id>=?1 AND id<?2 ORDER BY id LIMIT 32",
            )?;
            let mut rows = Vec::new();
            for row in
                statement.query_map(params![range.start as i64, range.end as i64], |row| {
                    let path = decode(&row.get::<_, Vec<u8>>(2)?);
                    let parent = path
                        .parent()
                        .and_then(|p| p.strip_prefix(&root).ok())
                        .map(|p| p.to_string_lossy().into_owned())
                        .filter(|p| !p.is_empty())
                        .unwrap_or_else(|| "此文件夹".into());
                    Ok(Row {
                        id: row.get::<_, i64>(0)? as usize,
                        name: row.get(1)?,
                        path,
                        parent,
                        directory: row.get(3)?,
                        group_heading: row.get(4)?,
                    })
                })?
            {
                rows.push(row?);
            }
            if current(s, r.token) {
                tx.send_blocking(crate::model::Event::Search(Notice::Page {
                    token: r.token,
                    serial,
                    rows,
                }))?;
            }
        }
        if last.elapsed() >= Duration::from_millis(100) || done {
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
        }
        if done {
            // Keep the completed result DB available for paged scrolling; do not emit idle notices.
            let mut pending = s.request.lock().unwrap();
            while pending.is_none() && s.page.lock().unwrap().is_none() && current(s, r.token) {
                pending = s.changed.wait(pending).unwrap();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Notice, Worker, matches};
    use crate::model::Event;
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
