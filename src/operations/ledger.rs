//! Durable, paged item results. A directory post callback is provisional until the batch closes.
use super::windows::{self as shell, Notice, Outcome};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[repr(i64)]
pub enum State {
    Pending = 0,
    Active = 1,
    Provisional = 2,
    Success = 3,
    Failed = 4,
    Skipped = 5,
    Canceled = 6,
    Partial = 7,
    Unknown = 8,
    NotProcessed = 9,
    Dispatched = 11,
}
impl State {
    fn from_i64(value: i64) -> Self {
        match value {
            0 => Self::Pending,
            1 => Self::Active,
            2 => Self::Provisional,
            3 => Self::Success,
            4 => Self::Failed,
            5 => Self::Skipped,
            6 => Self::Canceled,
            7 => Self::Partial,
            8 => Self::Unknown,
            9 => Self::NotProcessed,
            11 => Self::Dispatched,
            _ => Self::Unknown,
        }
    }
}
#[derive(Default, Debug)]
pub struct Counts {
    pub discovered: u64,
    pub successful: u64,
    pub failed: u64,
    pub skipped: u64,
    pub canceled: u64,
    pub partial: u64,
    pub unknown: u64,
    pub not_processed: u64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Row {
    pub sequence: u64,
    pub root: u64,
    pub path: PathBuf,
    pub state: State,
    pub hresult: Option<i32>,
    pub created: Option<PathBuf>,
}
pub struct Ledger {
    db: Connection,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArchiveIntent {
    pub target: crate::file_view::WinPath,
    pub stage: crate::file_view::WinPath,
    pub identity: Vec<u8>,
    pub extracted: bool,
    pub pid: u32,
    pub process_created: u64,
}
#[derive(serde::Serialize)]
pub struct RootRecord {
    pub key: u64,
    pub path: PathBuf,
    pub identity: Option<shell::identity::Identity>,
    pub directory: bool,
    pub bytes: Option<u64>,
    pub state: State,
}
pub const MAX_PAGE_BYTES: usize = 256 * 1024;
pub fn read_u64(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(column)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}
pub fn encode_path(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
pub fn decode_path(raw: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_wide(
        &raw.as_chunks::<2>()
            .0
            .iter()
            .map(|s| u16::from_le_bytes(*s))
            .collect::<Vec<_>>(),
    ))
}
fn terminal(result: i32, created: bool) -> State {
    match result as u32 {
        0 | 0x00270008 | 0x0027000a if created => State::Success,
        0x00270005 => State::Skipped,
        0x80004004 | 0x80270000 | 0x80270001 => State::Canceled,
        value if value & 0x80000000 != 0 => State::Failed,
        _ => State::Unknown,
    }
}
impl Ledger {
    pub fn read_only(path: &Path) -> anyhow::Result<Self> {
        let db = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.execute_batch("PRAGMA cache_size=-512")?;
        Ok(Self { db })
    }
    pub fn mark_dispatched(&self, keys: &[u64]) -> anyhow::Result<()> {
        self.begin()?;
        for key in keys {
            self.db.execute(
                "UPDATE results SET state=11 WHERE root=?1 AND root_item=1 AND state=0",
                [*key as i64],
            )?;
            self.db.execute(
                "UPDATE roots SET state=11 WHERE id=?1 AND state=0",
                [*key as i64],
            )?;
        }
        self.commit()
    }
    /// A worker error after dispatch cannot prove that the Shell made no changes.
    pub fn interrupt_unverified(&self) -> anyhow::Result<bool> {
        self.begin()?;
        let uncertain = self
            .db
            .execute("UPDATE results SET state=8 WHERE state IN(1,2,11)", [])?
            > 0;
        self.db
            .execute("UPDATE results SET state=9 WHERE state=0", [])?;
        self.db.execute("UPDATE roots SET state=(SELECT state FROM results WHERE root=roots.id AND root_item=1 LIMIT 1)", [])?;
        self.commit()?;
        Ok(uncertain)
    }
    pub fn visit_roots(
        &self,
        remaining: bool,
        mut visit: impl FnMut(RootRecord) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut query=self.db.prepare("SELECT id,path,identity,directory,bytes,state FROM roots WHERE(?1=0 OR state<>3)ORDER BY id")?;
        let rows = query.query_map([remaining], |r| {
            let path: Vec<u8> = r.get(1)?;
            let identity: Option<Vec<u8>> = r.get(2)?;
            let bytes: Option<i64> = r.get(4)?;
            Ok(RootRecord {
                key: r.get::<_, i64>(0)? as u64,
                path: decode_path(&path),
                identity: identity.and_then(|s| shell::identity::Identity::decode(&s)),
                directory: r.get(3)?,
                bytes: bytes.map(|n| n as u64),
                state: State::from_i64(r.get(5)?),
            })
        })?;
        for row in rows {
            visit(row?)?;
        }
        Ok(())
    }
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let db = Connection::open(path)?;
        Self::initialize(db)
    }
    fn initialize(db: Connection) -> anyhow::Result<Self> {
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA cache_size=-512; PRAGMA temp_store=FILE;
   CREATE TABLE IF NOT EXISTS roots(id INTEGER PRIMARY KEY,path BLOB NOT NULL,identity BLOB,directory INTEGER NOT NULL,bytes INTEGER,state INTEGER NOT NULL DEFAULT 0);
   CREATE TABLE IF NOT EXISTS results(seq INTEGER PRIMARY KEY,root INTEGER NOT NULL,root_item INTEGER NOT NULL,path BLOB NOT NULL,directory INTEGER,state INTEGER NOT NULL DEFAULT 0,hresult INTEGER,created BLOB,UNIQUE(root,path));
   CREATE INDEX IF NOT EXISTS result_root ON results(root,root_item);
   CREATE INDEX IF NOT EXISTS result_state ON results(state,seq);
   CREATE INDEX IF NOT EXISTS root_path ON roots(path);
   CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value BLOB NOT NULL);
   CREATE TABLE IF NOT EXISTS counters(state INTEGER PRIMARY KEY,count INTEGER NOT NULL);
   INSERT OR IGNORE INTO counters SELECT state,count(*)FROM results GROUP BY state;
   CREATE TRIGGER IF NOT EXISTS count_insert AFTER INSERT ON results BEGIN INSERT INTO counters VALUES(new.state,1)ON CONFLICT(state)DO UPDATE SET count=count+1; END;
   CREATE TRIGGER IF NOT EXISTS count_state AFTER UPDATE OF state ON results WHEN old.state!=new.state BEGIN UPDATE counters SET count=count-1 WHERE state=old.state; INSERT INTO counters VALUES(new.state,1)ON CONFLICT(state)DO UPDATE SET count=count+1; END;
   CREATE TRIGGER IF NOT EXISTS count_delete AFTER DELETE ON results BEGIN UPDATE counters SET count=count-1 WHERE state=old.state; END;")?;
        Ok(Self { db })
    }
    pub fn begin(&self) -> anyhow::Result<()> {
        if self.db.is_autocommit() {
            self.db.execute_batch("BEGIN IMMEDIATE")?;
        }
        Ok(())
    }
    pub fn commit(&self) -> anyhow::Result<()> {
        if !self.db.is_autocommit() {
            self.db.execute_batch("COMMIT")?;
        }
        Ok(())
    }
    pub fn archive_aborted(&self) -> anyhow::Result<()> {
        if !self.db.is_autocommit() {
            self.db.execute_batch("ROLLBACK")?;
        }
        self.db.execute_batch(
            "UPDATE roots SET state=9 WHERE state<>3;UPDATE results SET state=9 WHERE state<>3;",
        )?;
        Ok(())
    }
    pub fn add_archive_entry(
        &self,
        key: u64,
        path: &Path,
        directory: bool,
        bytes: u64,
    ) -> anyhow::Result<()> {
        let raw = encode_path(path);
        self.db.execute(
            "INSERT INTO roots(id,path,identity,directory,bytes)VALUES(?1,?2,NULL,?3,?4)",
            params![key as i64, &raw, directory, bytes as i64],
        )?;
        self.db.execute(
            "INSERT INTO results(root,root_item,path,directory)VALUES(?1,1,?2,?3)",
            params![key as i64, raw, directory],
        )?;
        Ok(())
    }
    pub fn archive_reset(&self) -> anyhow::Result<()> {
        self.db
            .execute_batch("DELETE FROM results;DELETE FROM roots;")?;
        Ok(())
    }
    pub fn archive_committed(&self, output: &Path, extracted: bool) -> anyhow::Result<()> {
        self.begin()?;
        self.db.execute("UPDATE roots SET state=3", [])?;
        self.db.execute(
            "UPDATE results SET state=3,hresult=0,created=CASE WHEN ?1 THEN path ELSE ?2 END",
            params![extracted, encode_path(output)],
        )?;
        self.db
            .execute("DELETE FROM meta WHERE key='archive-intent'", [])?;
        self.commit()?;
        Ok(())
    }
    pub fn archive_intent(&self, intent: &ArchiveIntent) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO meta VALUES('archive-intent',?1)",
            [serde_json::to_vec(intent)?],
        )?;
        Ok(())
    }
    pub fn pending_archive(&self) -> anyhow::Result<Option<ArchiveIntent>> {
        let json: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT value FROM meta WHERE key='archive-intent'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|v| serde_json::from_slice(&v).map_err(Into::into))
            .transpose()
    }
    pub fn add_root(
        &self,
        key: u64,
        path: &Path,
        metadata: Option<&shell::identity::Metadata>,
    ) -> anyhow::Result<()> {
        let identity = metadata.map(|m| m.identity.encode().to_vec());
        let directory = metadata.is_some_and(|m| m.directory);
        let bytes = metadata
            .and_then(|m| m.bytes)
            .and_then(|n| i64::try_from(n).ok());
        let raw = encode_path(path);
        self.db.execute(
            "INSERT INTO roots(id,path,identity,directory,bytes)VALUES(?1,?2,?3,?4,?5)",
            params![key as i64, raw, identity, directory, bytes],
        )?;
        self.db.execute(
            "INSERT INTO results(root,root_item,path,directory)VALUES(?1,1,?2,?3)",
            params![key as i64, raw, directory],
        )?;
        Ok(())
    }
    #[cfg(test)]
    pub fn identity(&self, key: u64) -> anyhow::Result<Option<shell::identity::Identity>> {
        let value: Option<Vec<u8>> = self.db.query_row(
            "SELECT identity FROM roots WHERE id=?1",
            [key as i64],
            |r| r.get(0),
        )?;
        Ok(value.and_then(|raw| shell::identity::Identity::decode(&raw)))
    }
    pub fn preparation_error(&self, key: u64, hresult: i32) -> anyhow::Result<()> {
        self.db.execute(
            "UPDATE results SET state=4,hresult=?2 WHERE root=?1 AND root_item=1",
            params![key as i64, hresult],
        )?;
        self.db
            .execute("UPDATE roots SET state=4 WHERE id=?1", [key as i64])?;
        Ok(())
    }
    pub fn record(&self, notice: &Notice) -> anyhow::Result<()> {
        match notice {
            Notice::Before {
                key,
                source: Some(path),
                root,
                ..
            } => {
                if *root {
                    self.db.execute(
                        "UPDATE results SET state=1 WHERE root=?1 AND root_item=1",
                        [*key as i64],
                    )?;
                    self.db
                        .execute("UPDATE roots SET state=1 WHERE id=?1", [*key as i64])?;
                } else {
                    let directory = shell::identity::capture(path).ok().map(|m| m.directory);
                    self.db.execute("INSERT INTO results(root,root_item,path,directory,state)VALUES(?1,0,?2,?3,1)ON CONFLICT(root,path)DO UPDATE SET state=1,directory=excluded.directory",params![*key as i64,encode_path(path),directory])?;
                }
            }
            Notice::After {
                key,
                source: Some(path),
                root,
                created,
                hresult,
            } => {
                let state = terminal(*hresult, created.is_some());
                let raw = encode_path(path);
                let created = created.as_ref().map(|p| encode_path(p));
                let row: Option<(i64, Option<bool>)> = if *root {
                    self.db
                        .query_row(
                            "SELECT seq,directory FROM results WHERE root=?1 AND root_item=1",
                            [*key as i64],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?
                } else {
                    self.db
                        .query_row(
                            "SELECT seq,directory FROM results WHERE root=?1 AND path=?2",
                            params![*key as i64, raw],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?
                };
                if let Some((seq, directory)) = row {
                    let state = if state == State::Success && directory != Some(false) {
                        State::Provisional
                    } else {
                        state
                    };
                    self.db.execute(
                        "UPDATE results SET state=?2,hresult=?3,created=?4 WHERE seq=?1",
                        params![seq, state as i64, *hresult, created],
                    )?;
                } else {
                    // A post without a pre is retained, but cannot establish a complete subtree.
                    let state = if state == State::Success {
                        State::Provisional
                    } else {
                        state
                    };
                    self.db.execute("INSERT INTO results(root,root_item,path,state,hresult,created)VALUES(?1,?2,?3,?4,?5,?6)ON CONFLICT(root,path)DO UPDATE SET state=excluded.state,hresult=excluded.hresult,created=excluded.created",params![*key as i64,*root,raw,state as i64,*hresult,created])?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    /// Close every declared root, including queue errors and unstarted work; never infer a rollback.
    pub fn finish_batch(
        &self,
        keys: &[u64],
        outcome: &Outcome,
        cancel_requested: bool,
    ) -> anyhow::Result<()> {
        self.begin()?;
        let result = (|| -> anyhow::Result<()> {
            for (key, error) in &outcome.queue_errors {
                self.db.execute(
                    "UPDATE results SET state=4,hresult=?2 WHERE root=?1 AND root_item=1",
                    params![*key as i64, *error],
                )?;
            }
            for key in keys {
                self.db.execute(
                    "UPDATE results SET state=9 WHERE root=?1 AND state IN(0,11)",
                    [*key as i64],
                )?;
                self.db.execute(
                    "UPDATE results SET state=8 WHERE root=?1 AND state=1",
                    [*key as i64],
                )?;
                let bad: i64 = self.db.query_row(
                    "SELECT count(*)FROM results WHERE root=?1 AND state IN(4,5,6,7,8,9)",
                    [*key as i64],
                    |r| r.get(0),
                )?;
                if bad == 0 && !outcome.aborted && outcome.perform_hresult >= 0 && !cancel_requested
                {
                    self.db.execute(
                        "UPDATE results SET state=3 WHERE root=?1 AND state=2",
                        [*key as i64],
                    )?;
                } else {
                    self.db.execute(
                        "UPDATE results SET state=8 WHERE root=?1 AND state=2 AND root_item=0",
                        [*key as i64],
                    )?;
                    // Directory creation can precede descendant work. Partial and unverified roots are not replayable failures.
                    self.db.execute("UPDATE results SET state=CASE WHEN ?2>0 THEN 7 ELSE 8 END WHERE root=?1 AND state=2 AND root_item=1",params![*key as i64,bad])?;
                }
                self.db.execute("UPDATE roots SET state=(SELECT state FROM results WHERE root=?1 AND root_item=1 LIMIT 1)WHERE id=?1",[*key as i64])?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = self.db.execute_batch("ROLLBACK");
            return result;
        }
        self.commit()
    }
    pub fn counts(&self) -> anyhow::Result<Counts> {
        let mut counts = Counts::default();
        let mut statement = self
            .db
            .prepare("SELECT state,count FROM counters WHERE count>0")?;
        for row in statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (state, count) = row?;
            let count = count as u64;
            counts.discovered += count;
            match State::from_i64(state) {
                State::Success => counts.successful = count,
                State::Failed => counts.failed = count,
                State::Skipped => counts.skipped = count,
                State::Canceled => counts.canceled = count,
                State::Partial => counts.partial = count,
                State::Unknown => counts.unknown = count,
                State::NotProcessed => counts.not_processed = count,
                _ => {}
            }
        }
        Ok(counts)
    }
    pub fn page(&self, after: u64, limit: usize, failed_only: bool) -> anyhow::Result<Vec<Row>> {
        let sql = if failed_only {
            "SELECT seq,root,path,state,hresult,created FROM results WHERE state=4 AND seq>?1 ORDER BY seq LIMIT ?2"
        } else {
            "SELECT seq,root,path,state,hresult,created FROM results WHERE seq>?1 ORDER BY seq LIMIT ?2"
        };
        let mut statement = self.db.prepare(sql)?;
        let mut page = Vec::new();
        let mut bytes = 0;
        for row in statement.query_map(params![after as i64, limit.min(96) as i64], |r| {
            let path: Vec<u8> = r.get(2)?;
            let created: Option<Vec<u8>> = r.get(5)?;
            let bytes = path.len() + created.as_ref().map_or(0, Vec::len);
            Ok((
                bytes,
                Row {
                    sequence: r.get::<_, i64>(0)? as u64,
                    root: r.get::<_, i64>(1)? as u64,
                    path: decode_path(&path),
                    state: State::from_i64(r.get(3)?),
                    hresult: r.get(4)?,
                    created: created.map(|s| decode_path(&s)),
                },
            ))
        })? {
            let (size, row) = row?;
            anyhow::ensure!(size <= MAX_PAGE_BYTES, "One result exceeds page budget");
            if bytes + size > MAX_PAGE_BYTES {
                break;
            }
            bytes += size;
            page.push(row);
        }
        Ok(page)
    }
    #[cfg(test)]
    pub fn retryable(&self, after: u64, limit: usize) -> anyhow::Result<Vec<Row>> {
        self.page(after, limit, true)
    }
    pub fn pending_batch(
        &self,
        next: u64,
        destination: &Path,
        new_name: Option<&OsString>,
    ) -> anyhow::Result<Vec<shell::Step>> {
        let mut query = self.db.prepare(
            "SELECT id,path,identity FROM roots WHERE id>=?1 AND state=0 ORDER BY id LIMIT 256",
        )?;
        let mut steps = Vec::new();
        let mut bytes = 0;
        let destination_bytes = destination.as_os_str().encode_wide().count() * 2
            + new_name.map_or(0, |s| s.encode_wide().count() * 2);
        for row in query.query_map([next as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })? {
            let (id, path, identity) = row?;
            let size = path.len() + destination_bytes;
            anyhow::ensure!(
                size <= shell::MAX_BATCH_PATH_BYTES,
                "One operation exceeds batch budget"
            );
            if bytes + size > shell::MAX_BATCH_PATH_BYTES {
                break;
            }
            bytes += size;
            steps.push(shell::Step {
                key: id as u64,
                source: decode_path(&path),
                destination: destination.to_owned(),
                new_name: new_name.cloned(),
                expected: identity.and_then(|s| shell::identity::Identity::decode(&s)),
                expected_destination: None,
            });
        }
        Ok(steps)
    }
    pub fn close_pending(&self) -> anyhow::Result<()> {
        self.db
            .execute("UPDATE results SET state=9 WHERE state=0", [])?;
        self.db
            .execute("UPDATE roots SET state=9 WHERE state=0", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ledger() -> Ledger {
        Ledger::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
    #[test]
    fn interrupted_dispatch_preserves_success_and_never_infers_rollback() {
        let ledger = ledger();
        for id in 0..4 {
            ledger
                .add_root(id, Path::new(&format!("D:/source/{id}")), None)
                .unwrap();
        }
        ledger
            .db
            .execute("UPDATE results SET state=3 WHERE root=0", [])
            .unwrap();
        ledger
            .db
            .execute("UPDATE results SET state=1 WHERE root=1", [])
            .unwrap();
        ledger.mark_dispatched(&[3]).unwrap();
        assert!(ledger.interrupt_unverified().unwrap());
        let rows = ledger.page(0, 96, false).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.state).collect::<Vec<_>>(),
            [
                State::Success,
                State::Unknown,
                State::NotProcessed,
                State::Unknown
            ]
        );
        assert!(ledger.retryable(0, 96).unwrap().is_empty());
    }
    fn notice(key: u64, root: bool, path: &str, result: i32, created: bool) -> Notice {
        Notice::After {
            key,
            root,
            source: Some(path.into()),
            hresult: result,
            created: created.then(|| PathBuf::from(format!("D:/target/{key}"))),
        }
    }
    fn outcome(aborted: bool) -> Outcome {
        Outcome {
            perform_hresult: 0,
            aborted,
            queued: 1,
            queue_errors: vec![],
        }
    }
    #[test]
    fn directory_post_is_provisional_and_child_failure_stays_partial() {
        let ledger = ledger();
        ledger
            .add_root(1, Path::new("D:/source/tree"), None)
            .unwrap();
        // The pre's directory metadata normally comes from the immutable manifest.
        ledger
            .db
            .execute("UPDATE results SET directory=1 WHERE root=1", [])
            .unwrap();
        ledger
            .record(&notice(1, true, "D:/source/tree", 0, true))
            .unwrap();
        assert_eq!(ledger.counts().unwrap().successful, 0);
        ledger
            .record(&notice(
                1,
                false,
                "D:/source/tree/locked.txt",
                0x80270027u32 as i32,
                false,
            ))
            .unwrap();
        ledger.finish_batch(&[1], &outcome(true), false).unwrap();
        let rows = ledger.page(0, 96, false).unwrap();
        assert_eq!(rows[0].state, State::Partial);
        assert_eq!(rows[1].state, State::Failed);
        let retry = ledger.retryable(0, 96).unwrap();
        assert_eq!(retry.len(), 1);
        assert!(retry[0].path.ends_with("locked.txt"));
    }
    #[test]
    fn cancellation_does_not_erase_success_or_replay_unstarted_roots() {
        let ledger = ledger();
        for key in 1..=3 {
            ledger
                .add_root(key, Path::new(&format!("D:/source/{key}")), None)
                .unwrap();
        }
        ledger
            .record(&notice(1, true, "D:/source/1", 0, true))
            .unwrap();
        ledger
            .record(&notice(2, true, "D:/source/2", 0x80270000u32 as i32, false))
            .unwrap();
        ledger
            .finish_batch(&[1, 2, 3], &outcome(true), true)
            .unwrap();
        let rows = ledger.page(0, 99, false).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.state).collect::<Vec<_>>(),
            [State::Success, State::Canceled, State::NotProcessed]
        );
        assert!(ledger.retryable(0, 96).unwrap().is_empty());
    }
    #[test]
    fn utf16_round_trip_keeps_unpaired_surrogate_and_non_bmp() {
        let path = PathBuf::from(OsString::from_wide(&[68, 58, 92, 0xd800, 0xd83d, 0xde00]));
        assert_eq!(decode_path(&encode_path(&path)), path);
    }
    #[test]
    fn failed_hresult_is_not_hidden_by_successful_perform_or_zero_queued() {
        let ledger = ledger();
        ledger.add_root(5, Path::new("D:/missing"), None).unwrap();
        let mut out = outcome(false);
        out.queued = 0;
        out.queue_errors.push((5, 0x80070002u32 as i32));
        ledger.finish_batch(&[5], &out, false).unwrap();
        assert_eq!(ledger.counts().unwrap().failed, 1);
    }
    #[test]
    fn result_pages_and_operation_batches_respect_utf16_byte_limits() {
        let ledger = ledger();
        for key in 0..10 {
            ledger
                .add_root(
                    key,
                    Path::new(&format!("D:/{}_{key}", "文".repeat(32000))),
                    None,
                )
                .unwrap();
        }
        let rows = ledger.page(0, usize::MAX, false).unwrap();
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .map(|r| encode_path(&r.path).len())
                .sum::<usize>()
                <= MAX_PAGE_BYTES
        );
        let batch = ledger
            .pending_batch(0, Path::new("D:/target"), None)
            .unwrap();
        assert_eq!(batch.len(), 4);
        assert_eq!(batch[0].key, 0);
        assert_eq!(
            ledger
                .pending_batch(4, Path::new("D:/target"), None)
                .unwrap()[0]
                .key,
            4
        );
    }
}
