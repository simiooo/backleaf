//! Latest-only logical file bytes; folder contents are never enumerated.
use crate::{
    model::{Event, FrozenSelection, Source},
    selection::SelectionMask,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub files: usize,
    pub bytes: u128,
    pub unknown: usize,
}
enum Snapshot {
    Directory(FrozenSelection),
    Fixture { count: usize, mask: SelectionMask },
}
struct Request {
    token: u64,
    generation: u64,
    snapshot: Snapshot,
}
struct Shared {
    pending: Mutex<Option<Request>>,
    changed: Condvar,
    latest: AtomicU64,
    stop: AtomicBool,
}
pub struct Stats {
    shared: Arc<Shared>,
    mask: Option<SelectionMask>,
    generation: u64,
    token: u64,
    pub result: Option<Result<Totals, String>>,
}
impl Stats {
    pub fn new(tx: async_channel::Sender<Event>) -> Self {
        let shared = Arc::new(Shared {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            latest: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });
        let state = shared.clone();
        std::thread::Builder::new()
            .name("backleaf-selection-bytes".into())
            .spawn(move || {
                loop {
                    let request = {
                        let mut pending = state.pending.lock().unwrap();
                        while pending.is_none() && !state.stop.load(Ordering::Acquire) {
                            pending = state.changed.wait(pending).unwrap();
                        }
                        if state.stop.load(Ordering::Acquire) {
                            return;
                        }
                        pending.take().unwrap()
                    };
                    let result =
                        sum(&request.snapshot, &state, request.token).map_err(|e| e.to_string());
                    if state.latest.load(Ordering::Acquire) == request.token
                        && !state.stop.load(Ordering::Acquire)
                        && tx
                            .send_blocking(Event::SelectionBytes {
                                generation: request.generation,
                                token: request.token,
                                result,
                            })
                            .is_err()
                    {
                        return;
                    }
                }
            })
            .expect("selection bytes worker");
        Self {
            shared,
            mask: None,
            generation: 0,
            token: 0,
            result: None,
        }
    }
    pub fn sync(&mut self, source: &Source, generation: u64, mask: &SelectionMask) {
        if generation == self.generation
            && self.mask.as_ref().is_some_and(|m| m.same_snapshot(mask))
        {
            return;
        }
        self.generation = generation;
        self.mask = Some(mask.clone());
        self.token += 1;
        self.result = None;
        let mut pending = self.shared.pending.lock().unwrap();
        self.shared.latest.store(self.token, Ordering::Release);
        *pending = None;
        if mask.count() > 0 {
            let snapshot = match source {
                Source::Fixture { count } => Some(Snapshot::Fixture {
                    count: *count,
                    mask: mask.clone(),
                }),
                _ => source
                    .freeze_selection(mask.clone())
                    .map(Snapshot::Directory),
            };
            if let Some(snapshot) = snapshot {
                *pending = Some(Request {
                    token: self.token,
                    generation,
                    snapshot,
                });
                self.shared.changed.notify_one();
            }
        }
    }
    pub fn accept(&mut self, generation: u64, token: u64, result: Result<Totals, String>) {
        if self.generation == generation && self.token == token {
            self.result = Some(result);
        }
    }
    pub fn suffix(&self) -> Option<String> {
        match &self.result {
            None => Some("…".into()),
            Some(Err(_)) => Some("大小不可用".into()),
            Some(Ok(t)) if t.files == 0 => None,
            Some(Ok(t)) if t.unknown == t.files => Some("大小不可用".into()),
            Some(Ok(t)) => Some(format!(
                "{}{}",
                crate::volume_capacity::bytes(t.bytes),
                if t.unknown > 0 { " + 未知" } else { "" }
            )),
        }
    }
}
impl Drop for Stats {
    fn drop(&mut self) {
        let _guard = self.shared.pending.lock().unwrap();
        self.shared.stop.store(true, Ordering::Release);
        self.shared.changed.notify_one();
    }
}
fn sum(snapshot: &Snapshot, state: &Arc<Shared>, token: u64) -> anyhow::Result<Totals> {
    let cancelled =
        || state.stop.load(Ordering::Acquire) || state.latest.load(Ordering::Acquire) != token;
    let mut result = Totals::default();
    let mut add = |bytes: Option<u64>| {
        result.files += 1;
        if let Some(bytes) = bytes {
            result.bytes += u128::from(bytes);
        } else {
            result.unknown += 1;
        }
    };
    match snapshot {
        Snapshot::Fixture { count, mask } if mask.count() == *count => {
            anyhow::ensure!(!cancelled(), "selection changed");
            let n = *count as u128;
            let folders = (n + 10) / 13;
            result.files = *count - folders as usize;
            result.bytes = (n * (n + 1) / 2
                - (3 * folders + 13 * folders * folders.saturating_sub(1) / 2))
                * 4096;
        }
        Snapshot::Fixture { count, mask } => {
            for id in mask.ids_from(0, *count) {
                anyhow::ensure!(!cancelled(), "selection changed");
                if id % 13 != 2 {
                    add(Some((id as u64 + 1) * 4096));
                }
            }
        }
        Snapshot::Directory(frozen) => {
            use rusqlite::{Connection, OpenFlags, OptionalExtension};
            let mut db = Connection::open_with_flags(
                &frozen.snapshot.database,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            db.busy_timeout(std::time::Duration::from_millis(300))?;
            let guard = state.clone();
            db.progress_handler(
                2048,
                Some(move || {
                    guard.stop.load(Ordering::Acquire)
                        || guard.latest.load(Ordering::Acquire) != token
                }),
            )?;
            if frozen.mask.count() > 32 && frozen.mask.count() > frozen.count / 8 {
                let mut query = db.prepare("SELECT id,bytes FROM files WHERE dir=0 AND id<?1")?;
                let mut rows = query.query([frozen.count as i64])?;
                while let Some(row) = rows.next()? {
                    anyhow::ensure!(!cancelled(), "selection changed");
                    if frozen.mask.contains(row.get::<_, i64>(0)? as usize) {
                        add(row.get::<_, Option<i64>>(1)?.map(|n| n as u64));
                    }
                }
            } else {
                let mut query = db.prepare("SELECT bytes FROM files WHERE dir=0 AND id=?1")?;
                for id in frozen.mask.ids_from(0, frozen.count) {
                    anyhow::ensure!(!cancelled(), "selection changed");
                    if let Some(bytes) = query
                        .query_row([id as i64], |r| r.get::<_, Option<i64>>(0))
                        .optional()?
                    {
                        add(bytes.map(|n| n as u64));
                    }
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_fixture_selection_handles_boundaries_and_ten_million_entries() {
        let state = shared();
        for count in 0..80 {
            let mut selected = crate::selection::Selection::default();
            selected.all(count);
            let total = sum(
                &Snapshot::Fixture {
                    count,
                    mask: selected.mask,
                },
                &state,
                1,
            )
            .unwrap();
            let expected = (0..count)
                .filter(|id| id % 13 != 2)
                .map(|id| (id as u128 + 1) * 4096)
                .sum::<u128>();
            assert_eq!(total.bytes, expected);
        }
        let mut selected = crate::selection::Selection::default();
        selected.all(10_000_000);
        let total = sum(
            &Snapshot::Fixture {
                count: 10_000_000,
                mask: selected.mask,
            },
            &state,
            1,
        )
        .unwrap();
        // Independent streaming oracle, evaluated outside this test: 10M fixture rows.
        assert_eq!(total.bytes, 189_046_175_901_540_352);
        assert_eq!(total.files, 9_230_769);
    }
    use std::{path::PathBuf, sync::OnceLock};
    fn shared() -> Arc<Shared> {
        Arc::new(Shared {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            latest: AtomicU64::new(1),
            stop: AtomicBool::new(false),
        })
    }
    fn frozen(path: PathBuf, count: usize) -> Snapshot {
        let mut selection = crate::selection::Selection::default();
        selection.all(count);
        Snapshot::Directory(FrozenSelection {
            snapshot: Arc::new(crate::model::DirectorySnapshot {
                database: path,
                parent: OnceLock::new(),
                parent_identity: OnceLock::new(),
            }),
            count,
            mask: selection.mask,
        })
    }
    #[test]
    fn unknown_sizes_and_large_file_totals_are_not_zeroed_or_overflowed() {
        let fixture = crate::test_support::Fixture::new("selection-bytes");
        let path = fixture.0.join("files.sqlite");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE files(id INTEGER PRIMARY KEY,dir INTEGER,bytes INTEGER);INSERT INTO files VALUES(0,0,-1),(1,0,-1),(2,0,NULL),(3,1,-1);").unwrap();
        let snapshot = frozen(path, 4);
        let state = shared();
        let total = sum(&snapshot, &state, 1).unwrap();
        assert_eq!(total.files, 3);
        assert_eq!(total.unknown, 1);
        assert_eq!(total.bytes, 2 * u128::from(u64::MAX));
        state.latest.store(2, Ordering::Release);
        assert!(sum(&snapshot, &state, 1).is_err());
        drop(db);
    }
    #[test]
    fn hundred_thousand_selection_totals_stream_without_names_or_recursion() {
        let fixture = crate::test_support::Fixture::new("selection-bytes-large");
        let path = fixture.0.join("files.sqlite");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE files(id INTEGER PRIMARY KEY,dir INTEGER,bytes INTEGER);WITH RECURSIVE n(i) AS(SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<99999) INSERT INTO files SELECT i,i%13=2,CASE WHEN i%17=0 THEN NULL ELSE 1024 END FROM n;").unwrap();
        let snapshot = frozen(path, 100_000);
        let started = std::time::Instant::now();
        let total = sum(&snapshot, &shared(), 1).unwrap();
        let files = (0..100_000).filter(|i| i % 13 != 2).count();
        let unknown = (0..100_000).filter(|i| i % 13 != 2 && i % 17 == 0).count();
        assert_eq!(total.files, files);
        assert_eq!(total.unknown, unknown);
        assert_eq!(total.bytes, ((files - unknown) * 1024) as u128);
        println!(
            "100000 selected entries, file bytes only: {:?}; constant-size result",
            started.elapsed()
        );
        drop(db);
    }
}
