//! Directory snapshots live on a worker-owned SQLite connection. The UI owns a viewport page.
use rusqlite::{Connection, params};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct Item {
    pub id: usize,
    pub name: String,
    pub label: String,
    pub directory: bool,
    pub bytes: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct Detail {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub bytes: Option<u64>,
}
pub enum Event {
    Session {
        locked: bool,
    },
    Count {
        generation: u64,
        count: usize,
        done: bool,
        error: Option<String>,
    },
    Page {
        generation: u64,
        serial: u64,
        items: Vec<Item>,
    },
    Detail {
        generation: u64,
        token: u64,
        value: Result<Detail, String>,
    },
}
#[derive(Default)]
pub(crate) struct Pending {
    page: Option<(u64, Range<usize>)>,
    detail: Option<(u64, usize)>,
}
pub enum Source {
    Fixture {
        count: usize,
    },
    Directory {
        count: usize,
        done: bool,
        pending: Arc<Mutex<Pending>>,
        stop: Arc<AtomicBool>,
        worker: thread::Thread,
    },
}
impl Drop for Source {
    fn drop(&mut self) {
        if let Self::Directory { stop, worker, .. } = self {
            stop.store(true, Ordering::Relaxed);
            worker.unpark();
        }
    }
}
impl Source {
    pub fn fixture(count: usize) -> Self {
        Self::Fixture { count }
    }
    pub fn count(&self) -> usize {
        match self {
            Self::Fixture { count } | Self::Directory { count, .. } => *count,
        }
    }
    pub fn directory(path: PathBuf, generation: u64, tx: async_channel::Sender<Event>) -> Self {
        let pending = Arc::new(Mutex::new(Pending::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (pending.clone(), stop.clone());
        let handle = thread::Builder::new().name("backleaf-directory".into()).spawn(move || {
            let cache = std::env::temp_dir().join("Backleaf");
            let unique=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let db_path = cache.join(format!("snapshot-{}-{generation}-{unique}.sqlite", std::process::id()));
            let result = (|| -> anyhow::Result<()> {
                std::fs::create_dir_all(&cache)?;
                let mut db = Connection::open(&db_path)?;
                db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-8192; CREATE TABLE files(id INTEGER PRIMARY KEY, short TEXT NOT NULL, raw BLOB NOT NULL, dir INTEGER NOT NULL, bytes INTEGER);")?;
                let mut entries = std::fs::read_dir(&path)?;
                let mut count = 0;
                let mut done = false;
                let mut last_notice = std::time::Instant::now();
                loop {
                    if s.load(Ordering::Relaxed) || tx.is_closed() { break; }
                    if !done {
                        let batch = db.transaction()?;
                        {
                            let mut insert = batch.prepare_cached("INSERT INTO files VALUES(?1,?2,?3,?4,?5)")?;
                            for _ in 0..512 {
                                match entries.next() {
                                    Some(Ok(entry)) => {
                                        let name = entry.file_name();
                                        let kind = entry.file_type().ok();
                                        let directory = kind.is_some_and(|t| t.is_dir());
                                        // No per-file metadata IO during enumeration. Sizes are requested in details.
                                        insert.execute(params![count as i64, short_name(&name.to_string_lossy(), 256), encode_name(&name), directory, Option::<i64>::None])?;
                                        count += 1;
                                    },
                                    Some(Err(error)) => { let _ = tx.send_blocking(Event::Count { generation, count, done: false, error: Some(error.to_string()) }); },
                                    None => { done = true; break; }
                                }
                            }
                        }
                        batch.commit()?;
                        if done || count == 512 || last_notice.elapsed().as_millis() >= 80 {
                            if tx.send_blocking(Event::Count { generation, count, done, error: None }).is_err() { break; }
                            last_notice = std::time::Instant::now();
                        }
                    }
                    let (page, detail) = { let mut lock = p.lock().unwrap(); (lock.page.take(), lock.detail.take()) };
                    if let Some((token, id)) = detail {
                        let result = (|| -> anyhow::Result<Detail> {
                            let raw: Vec<u8> = db.query_row("SELECT raw FROM files WHERE id=?1", [id as i64], |r| r.get(0))?;
                            let name = decode_name(&raw);
                            let full = path.join(&name);
                            let metadata = std::fs::metadata(&full)?;
                            Ok(Detail { id, name: name.to_string_lossy().into_owned(), path: full, directory: metadata.is_dir(), bytes: (!metadata.is_dir()).then_some(metadata.len()) })
                        })().map_err(|e| e.to_string());
                        if tx.send_blocking(Event::Detail { generation, token, value: result }).is_err() { break; }
                    }
                    if let Some((serial, range)) = page {
                        let mut query = db.prepare_cached("SELECT id,short,dir,bytes FROM files WHERE id>=?1 AND id<?2 ORDER BY id")?;
                        let items = query.query_map(params![range.start as i64, range.end as i64], |r| {
                            let name:String=r.get(1)?;let label=short_name(&name,96);
                            Ok(Item { id: r.get::<_, i64>(0)? as usize, name, label, directory: r.get(2)?, bytes: r.get::<_,Option<i64>>(3)?.map(|n|n as u64) })
                        })?.collect::<Result<Vec<_>, _>>()?;
                        if tx.send_blocking(Event::Page { generation, serial, items }).is_err() { break; }
                    }
                    if done { thread::park(); }
                }
                Ok(())
            })();
            if let Err(error) = result { let _ = tx.send_blocking(Event::Count { generation, count: 0, done: true, error: Some(error.to_string()) }); }
            // Only this process's disposable snapshot is removed, after SQLite closes.
            let _ = std::fs::remove_file(db_path);
        }).expect("directory worker");
        Self::Directory {
            count: 0,
            done: false,
            pending,
            stop,
            worker: handle.thread().clone(),
        }
    }
    pub fn request(&self, serial: u64, range: Range<usize>) {
        if let Self::Directory {
            pending, worker, ..
        } = self
        {
            pending.lock().unwrap().page = Some((serial, range));
            worker.unpark();
        }
    }
    pub fn detail(&self, token: u64, id: usize) -> Option<Detail> {
        match self {
            Self::Fixture { .. } => Some(fixture_detail(id)),
            Self::Directory {
                pending, worker, ..
            } => {
                pending.lock().unwrap().detail = Some((token, id));
                worker.unpark();
                None
            }
        }
    }
}
pub fn short_name(name: &str, limit: usize) -> String {
    let mut iter = name.graphemes(true);
    let mut value: String = iter.by_ref().take(limit).collect();
    if iter.next().is_some() {
        value.push('…');
    }
    value
}
pub fn fixture_item(id: usize) -> Item {
    let name = if id % 101 == 17 {
        format!("{id:08}_{}.txt", "W".repeat(240))
    } else {
        match id % 13 {
            0 => {
                format!("项目_{id:08}_关于城市与河流的研究记录_最终核对稿_👩🏽‍💻_家庭👨‍👩‍👧‍👦_与补充材料.pdf")
            }
            1 => format!(
                "{id:08}_{}_组合e\u{301}_日本語_한국어_العربية.txt",
                "无空格长文件名称".repeat(16)
            ),
            2 => format!("{id:08}_Photos_2026"),
            _ => format!("归档_{id:08}.pdf"),
        }
    };
    Item {
        id,
        label: short_name(&name, 96),
        name,
        directory: id % 13 == 2,
        bytes: Some((id as u64 + 1) * 4096),
    }
}
pub fn fixture_detail(id: usize) -> Detail {
    let item = fixture_item(id);
    let root = PathBuf::from("C:\\渲染基准").join("完整路径与名称".repeat(30));
    Detail {
        id,
        name: item.name.clone(),
        path: root.join(&item.name),
        directory: item.directory,
        bytes: item.bytes,
    }
}
#[cfg(windows)]
fn encode_name(name: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    name.encode_wide().flat_map(u16::to_le_bytes).collect()
}
#[cfg(windows)]
fn decode_name(raw: &[u8]) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(
        &raw.as_chunks::<2>()
            .0
            .iter()
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .collect::<Vec<_>>(),
    )
}

#[derive(Default)]
pub struct Selection {
    pub base: Range<usize>,
    pub extra: HashSet<usize>,
    pub excluded: HashSet<usize>,
    pub focus: Option<usize>,
    pub anchor: usize,
}
impl Selection {
    pub fn contains(&self, id: usize) -> bool {
        (self.base.contains(&id) || self.extra.contains(&id)) && !self.excluded.contains(&id)
    }
    pub fn count(&self) -> usize {
        self.base.len()
            + self
                .extra
                .iter()
                .filter(|id| !self.base.contains(id))
                .count()
            - self.excluded.len()
    }
    pub fn select(&mut self, id: usize, control: bool, shift: bool) {
        if shift {
            self.base = self.anchor.min(id)..self.anchor.max(id) + 1;
            self.extra.clear();
            self.excluded.clear();
        } else if control {
            if self.contains(id) {
                self.extra.remove(&id);
                if self.base.contains(&id) {
                    self.excluded.insert(id);
                }
            } else {
                self.excluded.remove(&id);
                self.extra.insert(id);
            }
            self.anchor = id;
        } else {
            self.base = id..id + 1;
            self.extra.clear();
            self.excluded.clear();
            self.anchor = id;
        }
        self.focus = Some(id);
    }
    pub fn all(&mut self, count: usize) {
        self.base = 0..count;
        self.extra.clear();
        self.excluded.clear();
    }
}
pub type PageCache = BTreeMap<usize, Arc<Item>>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn million_selection_is_constant_space_and_toggle_preserves_count() {
        let mut s = Selection::default();
        s.all(1_000_000);
        s.select(50, true, false);
        assert_eq!(s.count(), 999_999);
        assert!(!s.contains(50));
        s.select(50, true, false);
        assert_eq!(s.count(), 1_000_000);
        assert!(s.extra.len() <= 1);
    }
    #[test]
    fn truncation_keeps_graphemes_and_raw_value() {
        let raw = "👨‍👩‍👧‍👦e\u{301}中文";
        assert_eq!(short_name(raw, 2), "👨‍👩‍👧‍👦e\u{301}…");
        assert_eq!(raw.graphemes(true).count(), 4);
    }
    #[test]
    fn directory_snapshot_pages_and_details_keep_real_identity() {
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let name = format!(
            "backleaf-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let fixture = temp.join(&name);
        std::fs::create_dir(&fixture).unwrap();
        for i in 0..1600 {
            std::fs::write(fixture.join(format!("文件_{i:04}_👩🏽‍💻.txt")), i.to_string()).unwrap();
        }
        let (tx, rx) = async_channel::bounded(8);
        let source = Source::directory(fixture.clone(), 77, tx);
        let receive = || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                if let Ok(event) = rx.try_recv() {
                    return event;
                }
                assert!(std::time::Instant::now() < deadline, "worker timeout");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        loop {
            match receive() {
                Event::Count {
                    generation: 77,
                    count: 1600,
                    done: true,
                    error: None,
                } => break,
                Event::Count { error: Some(e), .. } => panic!("{e}"),
                _ => {}
            }
        }
        source.request(123, 1500..1532);
        let page = loop {
            if let Event::Page {
                generation: 77,
                serial: 123,
                items,
            } = receive()
            {
                break items;
            }
        };
        assert_eq!(page.len(), 32);
        assert_eq!(page[0].id, 1500);
        assert_eq!(page[31].id, 1531);
        source.detail(456, page[0].id);
        let detail = loop {
            if let Event::Detail {
                generation: 77,
                token: 456,
                value,
            } = receive()
            {
                break value.unwrap();
            }
        };
        assert_eq!(detail.name, page[0].name);
        assert!(detail.path.is_file());
        assert_eq!(
            detail.bytes,
            Some(std::fs::metadata(&detail.path).unwrap().len())
        );
        drop(source);
        drop(rx);
        // A uniquely created test directory, resolved and confined to the OS temp directory.
        let target = fixture.canonicalize().unwrap();
        assert!(target.starts_with(&temp));
        assert_eq!(target.file_name(), Some(std::ffi::OsStr::new(&name)));
        std::fs::remove_dir_all(target).unwrap();
    }
    #[test]
    fn utf16_name_roundtrip_does_not_replace_unpaired_surrogates() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let units = [0x4e2d, 0xd800, 0x0061, 0xdc00];
        let name = std::ffi::OsString::from_wide(&units);
        assert_eq!(
            decode_name(&encode_name(&name))
                .encode_wide()
                .collect::<Vec<_>>(),
            units
        );
    }
}
