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
    query: String,
}
struct Shared {
    request: Mutex<Option<Request>>,
    page: Mutex<Option<(u64, u64, Range<usize>)>>,
    changed: Condvar,
    stop: AtomicBool,
    latest: AtomicU64,
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
    pub fn new(tx: async_channel::Sender<crate::model::Event>) -> Self {
        let shared = Arc::new(Shared {
            request: Mutex::new(None),
            page: Mutex::new(None),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            latest: AtomicU64::new(0),
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
    pub fn search(&self, token: u64, root: PathBuf, query: String) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.page.lock().unwrap() = None;
        *self.0.request.lock().unwrap() = Some(Request { token, root, query });
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
    db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-512; CREATE TABLE frontier(id INTEGER PRIMARY KEY,path BLOB NOT NULL); CREATE TABLE results(id INTEGER PRIMARY KEY,name TEXT NOT NULL,path BLOB NOT NULL);")?;
    // Root may itself be a Known Folder redirection; never recurse through child reparse points.
    let root = r.root.canonicalize()?;
    let mut entries = Some(std::fs::read_dir(&root)?);
    let mut count = 0usize;
    let mut scanned = 0usize;
    let mut unavailable = 0usize;
    let mut skipped = 0usize;
    let mut done = false;
    let mut last = Instant::now() - Duration::from_secs(1);
    let query = r.query.trim().to_lowercase();
    while current(s, r.token) && !tx.is_closed() {
        if !done {
            let batch = db.transaction()?;
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
                        if matches(&name, &query) {
                            batch.execute(
                                "INSERT INTO results VALUES(?1,?2,?3)",
                                params![count as i64, name, encode(&path)],
                            )?;
                            count += 1;
                        }
                        match entry.file_type() {
                            Ok(kind) if kind.is_dir() => {
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
            batch.commit()?;
        }
        let page = s.page.lock().unwrap().take();
        if let Some((token, serial, range)) = page
            && token == r.token
        {
            let mut statement = db.prepare_cached(
                "SELECT id,name,path FROM results WHERE id>=?1 AND id<?2 ORDER BY id LIMIT 32",
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
                        assert_eq!(count, 140);
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
                        assert!(
                            rows.iter()
                                .all(|r| r.path.exists() && r.name.starts_with("报告"))
                        );
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
