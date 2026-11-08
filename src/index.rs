//! Persistent, ordinary-permission metadata index. No USN privileges or content upload.
use crate::operations::{
    ledger::{decode_path, encode_path},
    windows::identity,
};
use rusqlite::{Connection, OptionalExtension, params, types::Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub revision: u64,
    pub count: usize,
    pub building: bool,
    pub unavailable: usize,
    pub roots: Vec<PathBuf>,
    pub error: Option<String>,
}
pub enum Action {
    Add(PathBuf),
    AddOpportunistic(PathBuf),
    Touch(i64),
    SetPins(Vec<PathBuf>),
    Remove(PathBuf),
    Rescan,
}
pub struct Service(Arc<AtomicBool>, mpsc::SyncSender<Action>);
impl Drop for Service {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
pub fn database(directory: &Path) -> PathBuf {
    directory.join("file-index.sqlite")
}
pub fn open(directory: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(directory)?;
    let db = Connection::open(database(directory))?;
    db.busy_timeout(Duration::from_secs(2))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA cache_size=-8192; PRAGMA temp_store=FILE;
    CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL); INSERT OR IGNORE INTO meta VALUES('revision',0); INSERT OR IGNORE INTO meta VALUES('epoch',0);
    CREATE TABLE IF NOT EXISTS roots(id INTEGER PRIMARY KEY,path BLOB UNIQUE NOT NULL,count INTEGER NOT NULL DEFAULT 0,building INTEGER NOT NULL DEFAULT 1);
    CREATE TABLE IF NOT EXISTS objects(identity BLOB PRIMARY KEY,bytes INTEGER,modified INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS entries(id INTEGER PRIMARY KEY,root INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,parent BLOB NOT NULL,path BLOB NOT NULL,name TEXT NOT NULL,display TEXT NOT NULL,directory INTEGER NOT NULL,identity BLOB NOT NULL,epoch INTEGER NOT NULL,UNIQUE(root,path));
    CREATE INDEX IF NOT EXISTS entry_parent ON entries(root,parent); CREATE INDEX IF NOT EXISTS entry_identity ON entries(identity);
    CREATE TABLE IF NOT EXISTS frontier(root INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,path BLOB NOT NULL,crawl INTEGER NOT NULL,PRIMARY KEY(root,path));
    CREATE TABLE IF NOT EXISTS problems(root INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,path BLOB NOT NULL,error TEXT NOT NULL,PRIMARY KEY(root,path));
    CREATE TABLE IF NOT EXISTS grams(gram TEXT NOT NULL,id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,PRIMARY KEY(gram,id)); CREATE INDEX IF NOT EXISTS grams_entry ON grams(id);
    CREATE VIRTUAL TABLE IF NOT EXISTS names USING fts5(name,content='entries',content_rowid='id',tokenize='trigram');
    CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN INSERT INTO names(rowid,name)VALUES(new.id,new.name); END;
    CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN INSERT INTO names(names,rowid,name)VALUES('delete',old.id,old.name); END;
    CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE OF name ON entries BEGIN INSERT INTO names(names,rowid,name)VALUES('delete',old.id,old.name); INSERT INTO names(rowid,name)VALUES(new.id,new.name); END;")?;
    // These IDs are a bounded invalidation journal, not an OS change journal.
    // Triggers are in the same transaction as the indexed metadata mutation.
    db.execute_batch("CREATE TABLE IF NOT EXISTS search_changes(seq INTEGER PRIMARY KEY AUTOINCREMENT,entry INTEGER NOT NULL);
    CREATE TRIGGER IF NOT EXISTS search_entry_insert AFTER INSERT ON entries BEGIN INSERT INTO search_changes(entry) VALUES(new.id); END;
    CREATE TRIGGER IF NOT EXISTS search_entry_delete AFTER DELETE ON entries BEGIN INSERT INTO search_changes(entry) VALUES(old.id); END;
    CREATE TRIGGER IF NOT EXISTS search_entry_update AFTER UPDATE ON entries
    WHEN old.parent IS NOT new.parent OR old.path IS NOT new.path OR old.name IS NOT new.name OR old.display IS NOT new.display OR old.directory IS NOT new.directory OR old.identity IS NOT new.identity
    BEGIN INSERT INTO search_changes(entry) VALUES(old.id); INSERT INTO search_changes(entry) VALUES(new.id); END;
    CREATE TRIGGER IF NOT EXISTS search_object_update AFTER UPDATE ON objects WHEN old.bytes IS NOT new.bytes OR old.modified IS NOT new.modified
    BEGIN INSERT INTO search_changes(entry) SELECT id FROM entries WHERE identity=new.identity; END;
    CREATE TRIGGER IF NOT EXISTS search_changes_bound AFTER INSERT ON search_changes BEGIN DELETE FROM search_changes WHERE seq<=new.seq-8192; END;")?;
    ensure_root_columns(&db)?;
    Ok(db)
}
const MAX_ROOTS: usize = 64;
const MAX_OPPORTUNISTIC_ROOTS: usize = 8;
/// Add LRU/pin columns to databases built before opportunistic roots existed.
fn ensure_root_columns(db: &Connection) -> anyhow::Result<()> {
    let columns = db
        .prepare("PRAGMA table_info(roots)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "kind") {
        db.execute(
            "ALTER TABLE roots ADD COLUMN kind INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !columns.iter().any(|column| column == "used") {
        db.execute(
            "ALTER TABLE roots ADD COLUMN used INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    db.execute_batch("CREATE TABLE IF NOT EXISTS pins(path BLOB PRIMARY KEY);")?;
    Ok(())
}
fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
fn root_rows(db: &Connection) -> anyhow::Result<Vec<(i64, PathBuf, i64)>> {
    let mut query = db.prepare("SELECT id,path,kind FROM roots ORDER BY id")?;
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                decode_path(&row.get::<_, Vec<u8>>(1)?),
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
fn touch_root(db: &Connection, root: i64) -> anyhow::Result<()> {
    db.execute(
        "UPDATE roots SET used=?2 WHERE id=?1",
        params![root, unix_seconds()],
    )?;
    Ok(())
}
fn pinned_root_ids(db: &Connection) -> anyhow::Result<std::collections::HashSet<i64>> {
    let mut query = db.prepare("SELECT path FROM pins")?;
    let pins = query
        .query_map([], |row| row.get::<_, Vec<u8>>(0))?
        .map(|row| row.map(|raw| decode_path(&raw)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut pinned = std::collections::HashSet::new();
    for (id, root, _) in root_rows(db)? {
        let root = root.canonicalize().unwrap_or(root);
        if pins.iter().any(|pin| pin.starts_with(&root)) {
            pinned.insert(id);
        }
    }
    Ok(pinned)
}
/// Removes the least-recently-used unpinned opportunistic root. Returns false when
/// no eviction is possible (everything is permanent or pinned).
fn evict_lru_opportunistic(db: &Connection) -> anyhow::Result<bool> {
    let total =
        db.query_row("SELECT count(*) FROM roots", [], |row| row.get::<_, i64>(0))? as usize;
    let opportunistic = db.query_row("SELECT count(*) FROM roots WHERE kind=1", [], |row| {
        row.get::<_, i64>(0)
    })? as usize;
    if total < MAX_ROOTS && opportunistic < MAX_OPPORTUNISTIC_ROOTS {
        return Ok(false);
    }
    let pinned = pinned_root_ids(db)?;
    let mut query = db.prepare("SELECT id FROM roots WHERE kind=1 ORDER BY used,id")?;
    let candidates = query
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let Some(victim) = candidates.into_iter().find(|id| !pinned.contains(id)) else {
        return Ok(false);
    };
    db.execute("DELETE FROM roots WHERE id=?1", [victim])?;
    db.execute(
        "DELETE FROM objects WHERE NOT EXISTS(SELECT 1 FROM entries WHERE entries.identity=objects.identity)",
        [],
    )?;
    db.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    Ok(true)
}
fn ensure_root_capacity(db: &Connection) -> anyhow::Result<bool> {
    loop {
        let total =
            db.query_row("SELECT count(*) FROM roots", [], |row| row.get::<_, i64>(0))? as usize;
        let opportunistic = db.query_row("SELECT count(*) FROM roots WHERE kind=1", [], |row| {
            row.get::<_, i64>(0)
        })? as usize;
        if total < MAX_ROOTS && opportunistic < MAX_OPPORTUNISTIC_ROOTS {
            return Ok(true);
        }
        if !evict_lru_opportunistic(db)? {
            return Ok(false);
        }
    }
}
fn register_with_kind(db: &Connection, path: &Path, kind: i64) -> anyhow::Result<Option<i64>> {
    let path = path.canonicalize()?;
    anyhow::ensure!(path.is_dir(), "索引位置不是文件夹");
    if let Some((id, _, _)) = root_rows(db)?
        .into_iter()
        .find(|(_, root, _)| path.starts_with(root.canonicalize().unwrap_or_else(|_| root.clone())))
    {
        // Explicitly adding a folder promotes its covering opportunistic root to a
        // permanent one so LRU can never evict it later.
        if kind == 0 {
            db.execute(
                "UPDATE roots SET kind=0,used=?2 WHERE id=?1",
                params![id, unix_seconds()],
            )?;
        }
        touch_root(db, id)?;
        return Ok(Some(id));
    }
    if !ensure_root_capacity(db)? {
        return Ok(None);
    }
    let raw = encode_path(&path);
    db.execute(
        "INSERT OR IGNORE INTO roots(path,kind,used)VALUES(?1,?2,?3)",
        params![&raw, kind, unix_seconds()],
    )?;
    let root = db.query_row("SELECT id FROM roots WHERE path=?1", [&raw], |row| {
        row.get::<_, i64>(0)
    })?;
    enqueue(db, root, &path, true)?;
    Ok(Some(root))
}
/// On-demand root acquisition. Drive roots, temp trees and system install
/// locations are refused; the caller decides when a searched folder is worth
/// indexing. Returns the covering or newly-created root id, or None when capacity
/// cannot be made.
pub(crate) fn register_opportunistic(db: &Connection, path: &Path) -> anyhow::Result<Option<i64>> {
    let path = path.canonicalize()?;
    if !opportunistic_path_allowed(&path) {
        return Ok(None);
    }
    register_with_kind(db, &path, 1)
}
fn opportunistic_path_allowed(path: &Path) -> bool {
    if path.components().count() <= 2 {
        return false;
    }
    let temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    if path.starts_with(temp) {
        return false;
    }
    for variable in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        let Some(system) = std::env::var_os(variable) else {
            continue;
        };
        let system = PathBuf::from(system);
        let system = system.canonicalize().unwrap_or(system);
        if path.starts_with(system) {
            return false;
        }
    }
    true
}
/// Replace the saved-view pin set and evict unpinned LRU roots over budget.
pub(crate) fn set_pins(db: &mut Connection, pins: &[PathBuf]) -> anyhow::Result<()> {
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    txn.execute("DELETE FROM pins", [])?;
    for pin in pins {
        let path = pin.canonicalize().unwrap_or_else(|_| pin.clone());
        txn.execute(
            "INSERT OR IGNORE INTO pins VALUES(?1)",
            [encode_path(&path)],
        )?;
    }
    txn.commit()?;
    while evict_lru_opportunistic(db)? {}
    Ok(())
}
pub fn register(db: &Connection, path: &Path) -> anyhow::Result<()> {
    match register_with_kind(db, path, 0)? {
        Some(_) => Ok(()),
        None => anyhow::bail!("索引位置最多64个"),
    }
}
fn enqueue(db: &Connection, root: i64, path: &Path, crawl: bool) -> anyhow::Result<()> {
    let Some(registered) = db
        .query_row("SELECT path FROM roots WHERE id=?1", [root], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .optional()?
    else {
        return Ok(());
    };
    let registered = decode_path(&registered);
    let path = if path.starts_with(&registered) {
        path.to_owned()
    } else {
        path.canonicalize().unwrap_or_else(|_| path.to_owned())
    };
    if !path.starts_with(&registered) {
        return Ok(());
    }
    db.execute("INSERT INTO frontier VALUES(?1,?2,?3) ON CONFLICT(root,path)DO UPDATE SET crawl=max(crawl,excluded.crawl)",params![root,encode_path(&path),crawl])?;
    db.execute("UPDATE roots SET building=1 WHERE id=?1", [root])?;
    Ok(())
}
// Called only by the index writer, inside its transaction. Root spelling changes
// invalidate every old parent key, so rebuild that root rather than mixing aliases.
fn migrate_root_path(
    db: &Connection,
    root: i64,
    previous: &Path,
    next: &Path,
) -> anyhow::Result<()> {
    let stored = db
        .query_row("SELECT path FROM roots WHERE id=?1", [root], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .optional()?;
    if stored.as_deref() != Some(encode_path(previous).as_slice()) {
        return Ok(());
    }
    let raw = encode_path(next);
    if let Some(other) = db
        .query_row(
            "SELECT id FROM roots WHERE path=?1 AND id<>?2",
            params![&raw, root],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        db.execute("DELETE FROM roots WHERE id=?1", [root])?;
        enqueue(db, other, next, true)?;
    } else {
        db.execute("DELETE FROM frontier WHERE root=?1", [root])?;
        db.execute("DELETE FROM problems WHERE root=?1", [root])?;
        db.execute("DELETE FROM entries WHERE root=?1", [root])?;
        db.execute(
            "UPDATE roots SET path=?2,count=0,building=1 WHERE id=?1",
            params![root, &raw],
        )?;
        enqueue(db, root, next, true)?;
    }
    db.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    Ok(())
}
fn cjk(c: char) -> bool {
    matches!(c as u32,0x3400..=0x9fff|0xf900..=0xfaff|0x20000..=0x323af)
}
/// Short-query grams: every adjacent 2-character pair for arbitrary scripts,
/// plus single characters only for CJK (non-CJK 1-character queries stay on the
/// `instr` fallback because their gram lists are broad and costly to materialize).
/// New databases get the full set; `migrate_short_grams` backfills pair grams for
/// databases built by the earlier CJK-only version.
pub fn short_grams(name: &str) -> std::collections::BTreeSet<String> {
    let chars = name.chars().collect::<Vec<_>>();
    let mut set = std::collections::BTreeSet::new();
    for (i, &ch) in chars.iter().enumerate() {
        if cjk(ch) {
            set.insert(ch.to_string());
        }
        if let Some(&next) = chars.get(i + 1) {
            set.insert(format!("{ch}{next}"));
        }
    }
    set
}
fn insert_short_grams(db: &Connection, id: i64, name: &str) -> anyhow::Result<()> {
    for gram in short_grams(name) {
        db.execute(
            "INSERT OR IGNORE INTO grams VALUES(?1,?2)",
            params![gram, id],
        )?;
    }
    Ok(())
}
/// Whether `grams` contains the extended 2-character pair set (not only CJK).
pub fn grams_ready(db: &Connection) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key='grams_v2')",
        [],
        |row| row.get(0),
    )
    .unwrap_or(false)
}
/// One-time backfill for databases built by the CJK-only gram version. Runs on the
/// writer after it acquires the lock; readers keep using the old `instr` fallback
/// until the flag and rows are committed.
fn migrate_short_grams(db: &mut Connection) -> anyhow::Result<()> {
    if grams_ready(db) {
        return Ok(());
    }
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let entries = {
        let mut query = txn.prepare("SELECT id,name FROM entries")?;
        query
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, name) in entries {
        insert_short_grams(&txn, id, &name)?;
    }
    txn.execute("INSERT OR IGNORE INTO meta VALUES('grams_v2',1)", [])?;
    txn.commit()?;
    Ok(())
}
pub struct IndexedItem {
    pub id: i64,
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub identity: Vec<u8>,
}
/// Search scope for one folder inside a registered index root.
#[derive(Clone, Debug)]
pub struct Scope {
    pub root: i64,
    pub path: Vec<u8>,
    pub recursive: bool,
}
impl Scope {
    /// UTF-16LE byte range covering every path below `path` without matching
    /// the folder itself. Directory-entry paths always use `\` and never `/`.
    fn range(&self) -> (Vec<u8>, Vec<u8>) {
        const SEPARATOR: [u8; 2] = [b'\\', 0];
        const SUCCESSOR: [u8; 2] = [b']', 0];
        if self.path.ends_with(&SEPARATOR) {
            // Drive roots already end in a separator; children are `path + name`.
            let mut upper = self.path[..self.path.len() - 2].to_vec();
            upper.extend(SUCCESSOR);
            (self.path.clone(), upper)
        } else {
            let mut lower = self.path.clone();
            lower.extend(SEPARATOR);
            let mut upper = lower.clone();
            let tail = upper.len() - 2;
            upper[tail..].copy_from_slice(&SUCCESSOR);
            (lower, upper)
        }
    }
}
pub(crate) fn components_cover(root: &Path, path: &Path) -> bool {
    let mut root = root.components();
    let mut path = path.components();
    loop {
        match (root.next(), path.next()) {
            (None, _) => return true,
            (Some(expected), Some(actual)) => {
                if expected.as_os_str().to_string_lossy().to_lowercase()
                    != actual.as_os_str().to_string_lossy().to_lowercase()
                {
                    return false;
                }
            }
            (Some(_), None) => return false,
        }
    }
}
pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    left.components().count() == right.components().count() && components_cover(left, right)
}
pub(crate) fn inside(folder: &Path, path: &Path, recursive: bool) -> bool {
    if recursive {
        path != folder && components_cover(folder, path)
    } else {
        path.parent()
            .is_some_and(|parent| same_path(parent, folder))
    }
}
/// Read-only connection for search planning; never creates a missing index.
pub fn open_reader(directory: &Path) -> anyhow::Result<Connection> {
    let db = Connection::open_with_flags(
        database(directory),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(Duration::from_secs(2))?;
    Ok(db)
}
/// Decide whether a default search can be answered from the persistent index.
///
/// It is deliberately conservative: the covering root must be fully built, its
/// root path must not be marked unavailable, and the searched folder must already
/// be an indexed directory (identity-matched, so spelling and case do not matter).
/// Anything else returns `None` and the caller keeps the live filesystem scan.
pub fn plan_scope(db: &Connection, path: &Path, recursive: bool) -> anyhow::Result<Option<Scope>> {
    let actual = path.canonicalize()?;
    let mut q = db.prepare("SELECT id,path FROM roots WHERE building=0 ORDER BY id")?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(q);
    let mut best: Option<(usize, i64, PathBuf, Vec<u8>)> = None;
    for (id, raw) in rows {
        let root = decode_path(&raw);
        if !components_cover(&root, &actual) {
            continue;
        }
        let components = root.components().count();
        if best.as_ref().is_none_or(|(count, ..)| components > *count) {
            best = Some((components, id, root, raw));
        }
    }
    let Some((_, root_id, root, root_raw)) = best else {
        return Ok(None);
    };
    let blocked: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM problems WHERE root=?1 AND path=?2)",
        params![root_id, &root_raw],
        |r| r.get(0),
    )?;
    if blocked {
        return Ok(None);
    }
    let scope_path = if same_path(&root, &actual) {
        root_raw
    } else {
        let Ok(metadata) = identity::capture(&actual) else {
            return Ok(None);
        };
        let Some(stored) = db
            .query_row(
                "SELECT path FROM entries WHERE root=?1 AND identity=?2 AND directory=1",
                params![root_id, metadata.identity.encode().to_vec()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        stored
    };
    // A pending rename keeps the same identity under an old path. Fall back to the
    // live scan until the index records the new spelling instead of returning rows
    // that no longer resolve.
    if !same_path(&decode_path(&scope_path), &actual) {
        return Ok(None);
    }
    Ok(Some(Scope {
        root: root_id,
        path: scope_path,
        recursive,
    }))
}
#[derive(Clone, Copy)]
enum MatchKind {
    Grams,
    Trigram,
    Instr,
}
fn classify(db: &Connection, query: &str) -> (MatchKind, String) {
    let lower = query.to_lowercase();
    let n = lower.chars().count();
    let pair_grams = n == 2 && grams_ready(db);
    if (1..=2).contains(&n) && (pair_grams || lower.chars().all(cjk)) {
        (MatchKind::Grams, lower)
    } else if n >= 3 {
        (
            MatchKind::Trigram,
            format!("\"{}\"", lower.replace('"', "\"\"")),
        )
    } else {
        (MatchKind::Instr, lower)
    }
}
fn matched_cte(kind: MatchKind, cursor: bool) -> &'static str {
    match (kind, cursor) {
        (MatchKind::Grams, true) => "SELECT id FROM grams WHERE gram=? AND id>?",
        (MatchKind::Grams, false) => "SELECT id FROM grams WHERE gram=?",
        (MatchKind::Trigram, true) => {
            "SELECT rowid AS id FROM names WHERE names MATCH ? AND rowid>?"
        }
        (MatchKind::Trigram, false) => "SELECT rowid AS id FROM names WHERE names MATCH ?",
        (MatchKind::Instr, _) => "",
    }
}
fn scope_conditions(scope: Option<&Scope>, recursive: bool, values: &mut Vec<Value>) -> String {
    match scope {
        Some(scope) => {
            let mut sql = String::from(" AND e.root=?");
            values.push(Value::Integer(scope.root));
            if scope.recursive {
                let (lower, upper) = scope.range();
                sql.push_str(" AND e.path>=? AND e.path<?");
                values.push(Value::Blob(lower));
                values.push(Value::Blob(upper));
            } else {
                sql.push_str(" AND e.parent=?");
                values.push(Value::Blob(scope.path.clone()));
            }
            sql
        }
        None if !recursive => " AND e.parent IN(SELECT path FROM roots)".into(),
        None => String::new(),
    }
}
/// Upper-bound match count for a scoped query, stopping at `cap`. Used by the
/// planner to keep broad queries on the live scan instead of paying an identity
/// check for every match.
pub fn estimate_matches(
    db: &Connection,
    query: &str,
    scope: &Scope,
    cap: usize,
) -> anyhow::Result<usize> {
    use rusqlite::params_from_iter;
    let (kind, value) = classify(db, query);
    let mut values = vec![Value::Text(value)];
    let mut sql = match kind {
        MatchKind::Instr => {
            String::from("SELECT count(*) FROM (SELECT 1 FROM entries e WHERE instr(e.name,?)>0")
        }
        kind => format!(
            "WITH matched AS MATERIALIZED ({}) SELECT count(*) FROM (SELECT 1 FROM matched JOIN entries e ON e.id=matched.id WHERE 1=1",
            matched_cte(kind, false)
        ),
    };
    let conditions = scope_conditions(Some(scope), scope.recursive, &mut values);
    sql.push_str(&conditions);
    sql.push_str(" LIMIT ?)");
    values.push(Value::Integer(cap as i64));
    let count = db.query_row(&sql, params_from_iter(values), |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as usize)
}
pub fn scope_size(db: &Connection, scope: &Scope) -> anyhow::Result<usize> {
    use rusqlite::params_from_iter;
    let mut values = Vec::new();
    let conditions = scope_conditions(Some(scope), scope.recursive, &mut values);
    let sql = format!("SELECT count(*) FROM entries e WHERE 1=1{conditions}");
    let count = db.query_row(&sql, params_from_iter(values), |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as usize)
}
pub fn query_batch(
    db: &Connection,
    query: &str,
    after: i64,
    recursive: bool,
    scope: Option<&Scope>,
) -> anyhow::Result<Vec<IndexedItem>> {
    use rusqlite::params_from_iter;
    let (kind, value) = classify(db, query);
    let fields = "SELECT e.id,e.display,e.path,e.directory,e.identity";
    let mut values = vec![Value::Text(value), Value::Integer(after)];
    // A scoped lookup must let the inverted index drive the join. Letting SQLite
    // scan the scoped entry range first makes it evaluate MATCH for every row,
    // which is correct but orders of magnitude slower.
    let mut sql = match kind {
        MatchKind::Instr => {
            format!("{fields} FROM entries e WHERE instr(e.name,?)>0 AND e.id>?")
        }
        kind => match scope {
            Some(_) => format!(
                "WITH matched AS MATERIALIZED ({}) {fields} FROM matched JOIN entries e ON e.id=matched.id WHERE 1=1",
                matched_cte(kind, true)
            ),
            None if matches!(kind, MatchKind::Trigram) => format!(
                "{fields} FROM names JOIN entries e ON e.id=names.rowid WHERE names MATCH ? AND names.rowid>?"
            ),
            None => format!(
                "{fields} FROM grams g JOIN entries e ON e.id=g.id WHERE g.gram=? AND g.id>?"
            ),
        },
    };
    let conditions = scope_conditions(scope, recursive, &mut values);
    sql.push_str(&conditions);
    sql.push_str(" ORDER BY e.id LIMIT 256");
    let mut q = db.prepare(&sql)?;
    let rows = q.query_map(params_from_iter(values), |r| {
        Ok(IndexedItem {
            id: r.get(0)?,
            name: r.get(1)?,
            path: decode_path(&r.get::<_, Vec<u8>>(2)?),
            directory: r.get(3)?,
            identity: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
fn erase_missing(db: &Connection, root: i64, parent: &Path, epoch: i64) -> anyhow::Result<usize> {
    Ok(db.execute("WITH RECURSIVE gone(path) AS(SELECT path FROM entries WHERE root=?1 AND parent=?2 AND epoch<>?3 UNION ALL SELECT e.path FROM entries e JOIN gone g ON e.parent=g.path WHERE e.root=?1)DELETE FROM entries WHERE root=?1 AND path IN(SELECT path FROM gone)",params![root,encode_path(parent),epoch])?)
}
fn erase_path(db: &Connection, root: i64, path: &Path) -> anyhow::Result<()> {
    let raw = encode_path(path);
    let mut prefix = raw.clone();
    if !prefix.ends_with(&[b'\\', 0]) {
        prefix.extend([b'\\', 0]);
    }
    db.execute(
        "DELETE FROM problems WHERE root=?1 AND (path=?2 OR substr(path,1,length(?3))=?3)",
        params![root, &raw, &prefix],
    )?;
    let removed=db.execute("WITH RECURSIVE gone(path)AS(SELECT ?2 UNION ALL SELECT e.path FROM entries e JOIN gone g ON e.parent=g.path WHERE e.root=?1)DELETE FROM entries WHERE root=?1 AND path IN(SELECT path FROM gone)",params![root,encode_path(path)])?;
    db.execute(
        "UPDATE roots SET count=max(0,count-?2) WHERE id=?1",
        params![root, removed as i64],
    )?;
    Ok(())
}
/// Test-only cost counters for the maintenance benchmark. Production builds omit
/// this module and every instrumentation branch below.
#[cfg(test)]
pub(crate) mod profile {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;
    pub static ENUMERATE_NS: AtomicU64 = AtomicU64::new(0);
    pub static IDENTITY_NS: AtomicU64 = AtomicU64::new(0);
    pub static UPSERT_NS: AtomicU64 = AtomicU64::new(0);
    pub static INCREMENTAL_CAPTURE_NS: AtomicU64 = AtomicU64::new(0);
    pub static INCREMENTAL_ROOT_NS: AtomicU64 = AtomicU64::new(0);
    pub static INCREMENTAL_UPSERT_NS: AtomicU64 = AtomicU64::new(0);
    pub static INCREMENTAL_COMMIT_NS: AtomicU64 = AtomicU64::new(0);
    pub static INCREMENTAL_SQL_NS: AtomicU64 = AtomicU64::new(0);
    pub static RECONCILED_ENTRIES: AtomicU64 = AtomicU64::new(0);
    pub fn reset() {
        for counter in [
            &ENUMERATE_NS,
            &IDENTITY_NS,
            &UPSERT_NS,
            &INCREMENTAL_CAPTURE_NS,
            &INCREMENTAL_ROOT_NS,
            &INCREMENTAL_UPSERT_NS,
            &INCREMENTAL_COMMIT_NS,
            &INCREMENTAL_SQL_NS,
            &RECONCILED_ENTRIES,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
    pub fn add(counter: &AtomicU64, start: Instant) {
        counter.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
    pub fn millis(counter: &AtomicU64) -> f64 {
        (counter.load(Ordering::Relaxed) as f64 / 1_000_000.).min(f64::MAX)
    }
    pub fn entries() -> u64 {
        RECONCILED_ENTRIES.load(Ordering::Relaxed)
    }
}
fn upsert(
    db: &Connection,
    root: i64,
    parent: &Path,
    path: &Path,
    metadata: &identity::Metadata,
    epoch: i64,
    crawl: bool,
) -> anyhow::Result<()> {
    let raw = encode_path(path);
    db.execute(
        "DELETE FROM problems WHERE root=?1 AND path=?2",
        params![root, &raw],
    )?;
    let identity = metadata.identity.encode();
    let display = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let name = display.to_lowercase();
    let old = db
        .query_row(
            "SELECT id,identity FROM entries WHERE root=?1 AND path=?2",
            params![root, &raw],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    db.execute("INSERT INTO objects VALUES(?1,?2,?3) ON CONFLICT(identity)DO UPDATE SET bytes=excluded.bytes,modified=excluded.modified WHERE objects.bytes IS NOT excluded.bytes OR objects.modified<>excluded.modified",params![identity.as_slice(),metadata.bytes.map(|n|n as i64),metadata.modified as i64])?;
    db.execute("INSERT INTO entries(root,parent,path,name,display,directory,identity,epoch)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)ON CONFLICT(root,path)DO UPDATE SET parent=excluded.parent,directory=excluded.directory,identity=excluded.identity,epoch=excluded.epoch",params![root,encode_path(parent),&raw,name,display,metadata.directory,identity.as_slice(),epoch])?;
    if old.is_none() {
        let id = db.last_insert_rowid();
        db.execute("UPDATE roots SET count=count+1 WHERE id=?1", [root])?;
        insert_short_grams(db, id, &name)?;
    }
    let descend = metadata.directory
        && !metadata.reparse
        && metadata.attributes & (0x1000 | 0x40000 | 0x400000) == 0;
    if descend
        && (crawl
            || old
                .as_ref()
                .is_none_or(|(_, old)| old.as_slice() != identity.as_slice()))
    {
        enqueue(db, root, path, true)?;
    } else if !descend {
        let removed = erase_missing(db, root, path, -1)?;
        db.execute(
            "UPDATE roots SET count=max(0,count-?2)WHERE id=?1",
            params![root, removed as i64],
        )?;
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn incremental(
    db: &mut Connection,
    root: i64,
    path: &Path,
    excluded: &Path,
) -> anyhow::Result<()> {
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    #[cfg(test)]
    let sql_start = Instant::now();
    incremental_in_txn(&txn, root, path, None, excluded)?;
    txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    #[cfg(test)]
    let commit_start = Instant::now();
    txn.commit()?;
    #[cfg(test)]
    {
        crate::index::profile::add(&crate::index::profile::INCREMENTAL_COMMIT_NS, commit_start);
        crate::index::profile::add(&crate::index::profile::INCREMENTAL_SQL_NS, sql_start);
    }
    Ok(())
}

/// Apply a drained notification batch in one transaction. The caller collapses
/// duplicate paths; a paired rename (`from` = old name) erases the old row in the
/// same transaction instead of waiting for a separate delete notification.
pub(crate) fn incremental_batch(
    db: &mut Connection,
    changes: &[crate::index_watch::Change],
    excluded: &Path,
) -> anyhow::Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    #[cfg(test)]
    let sql_start = Instant::now();
    for change in changes {
        incremental_in_txn(
            &txn,
            change.root,
            &change.path,
            change.from.as_deref(),
            excluded,
        )?;
    }
    txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    #[cfg(test)]
    let commit_start = Instant::now();
    txn.commit()?;
    #[cfg(test)]
    {
        crate::index::profile::add(&crate::index::profile::INCREMENTAL_COMMIT_NS, commit_start);
        crate::index::profile::add(&crate::index::profile::INCREMENTAL_SQL_NS, sql_start);
    }
    Ok(())
}

fn incremental_in_txn(
    txn: &rusqlite::Transaction<'_>,
    root: i64,
    path: &Path,
    rename_from: Option<&Path>,
    excluded: &Path,
) -> anyhow::Result<()> {
    if path.starts_with(excluded)
        || path
            .components()
            .any(|c| crate::operations::archive::temporary_name(c.as_os_str()))
    {
        return Ok(());
    }
    #[cfg(test)]
    let root_start = Instant::now();
    let Some(registered) = txn
        .query_row("SELECT path FROM roots WHERE id=?1", [root], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .optional()?
    else {
        return Ok(());
    };
    let registered_path = decode_path(&registered);
    let actual_root = registered_path
        .canonicalize()
        .unwrap_or_else(|_| registered_path.clone());
    #[cfg(test)]
    crate::index::profile::add(&crate::index::profile::INCREMENTAL_ROOT_NS, root_start);
    if actual_root != registered_path {
        migrate_root_path(txn, root, &registered_path, &actual_root)?;
        return Ok(());
    }
    if let Some(old) = rename_from
        && old != path
    {
        erase_path(txn, root, old)?;
    }
    #[cfg(test)]
    let capture_start = Instant::now();
    let captured = identity::capture(path);
    #[cfg(test)]
    crate::index::profile::add(
        &crate::index::profile::INCREMENTAL_CAPTURE_NS,
        capture_start,
    );
    match captured {
        Ok(metadata) => {
            let epoch = txn.query_row("SELECT value FROM meta WHERE key='epoch'", [], |r| {
                r.get::<_, i64>(0)
            })?;
            // An old-name notification can still resolve after a case-only
            // rename on Windows. Use the actual spelling, otherwise two UTF-16
            // keys can describe one directory entry indefinitely.
            let normalized =
                if !metadata.reparse && metadata.attributes & (0x1000 | 0x40000 | 0x400000) == 0 {
                    path.canonicalize().unwrap_or_else(|_| path.to_owned())
                } else {
                    path.to_owned()
                };
            if normalized != path {
                erase_path(txn, root, path)?;
            }
            if normalized.starts_with(decode_path(&registered))
                && let Some(parent) = normalized.parent()
            {
                #[cfg(test)]
                let upsert_start = Instant::now();
                upsert(txn, root, parent, &normalized, &metadata, epoch, false)?;
                #[cfg(test)]
                crate::index::profile::add(
                    &crate::index::profile::INCREMENTAL_UPSERT_NS,
                    upsert_start,
                );
                if metadata.reparse || metadata.attributes & (0x1000 | 0x40000 | 0x400000) != 0 {
                    enqueue(txn, root, parent, false)?;
                }
            }
        }
        Err(error) => {
            erase_path(txn, root, path)?;
            if path.exists() {
                txn.execute(
                    "INSERT OR REPLACE INTO problems VALUES(?1,?2,?3)",
                    params![root, encode_path(path), error.to_string()],
                )?;
            }
        }
    }
    Ok(())
}
enum DirSource {
    System(Box<std::fs::ReadDir>),
    Bulk(crate::index_bulk::DirectoryReader),
}
impl DirSource {
    /// `Some(metadata)` comes from bulk enumeration; `None` means the caller must
    /// capture identity itself (ordinary read_dir fallback).
    fn next(&mut self) -> anyhow::Result<Option<(PathBuf, Option<identity::Metadata>)>> {
        match self {
            Self::System(entries) => Ok(entries
                .next()
                .transpose()?
                .map(|entry| (entry.path(), None))),
            Self::Bulk(reader) => Ok(reader
                .next()?
                .map(|entry| (entry.path, Some(entry.metadata)))),
        }
    }
}
const FAST_PATH_LIMIT: usize = 65_536;

struct OldChild {
    path: Vec<u8>,
    display: String,
    directory: bool,
    identity: Vec<u8>,
    bytes: Option<i64>,
    modified: Option<i64>,
}

fn old_children(
    db: &Connection,
    root: i64,
    parent: &Path,
    limit: usize,
) -> anyhow::Result<Option<Vec<OldChild>>> {
    let parent = encode_path(parent);
    let count: i64 = db.query_row(
        "SELECT count(*) FROM entries WHERE root=?1 AND parent=?2",
        params![root, &parent],
        |row| row.get(0),
    )?;
    if count as usize > limit {
        return Ok(None);
    }
    let mut query = db.prepare(
        "SELECT e.path,e.display,e.directory,e.identity,o.bytes,o.modified \
         FROM entries e LEFT JOIN objects o ON o.identity=e.identity \
         WHERE e.root=?1 AND e.parent=?2",
    )?;
    let children = query
        .query_map(params![root, &parent], |row| {
            Ok(OldChild {
                path: row.get(0)?,
                display: row.get(1)?,
                directory: row.get(2)?,
                identity: row.get(3)?,
                bytes: row.get(4)?,
                modified: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(children))
}

fn unchanged_directory(old: &[OldChild], new: &[(PathBuf, identity::Metadata)]) -> bool {
    if old.len() != new.len() {
        return false;
    }
    let mut remaining: std::collections::HashMap<&[u8], usize> = old
        .iter()
        .enumerate()
        .map(|(index, child)| (child.path.as_slice(), index))
        .collect();
    for (path, metadata) in new {
        let raw = encode_path(path);
        let Some(index) = remaining.remove(raw.as_slice()) else {
            return false;
        };
        let child = &old[index];
        let display = path.file_name().unwrap_or_default().to_string_lossy();
        if child.display != display.as_ref()
            || child.directory != metadata.directory
            || child.identity != metadata.identity.encode()
            || child.bytes != metadata.bytes.map(|bytes| bytes as i64)
            || child.modified != Some(metadata.modified as i64)
        {
            return false;
        }
    }
    remaining.is_empty()
}

/// Directory-level keyed diff: every direct child matches an indexed row with the
/// same identity/metadata, so no entry rows need to be rewritten. Only the
/// descendants that a crawl still has to visit and this directory's frontier row are
/// touched.
fn finish_unchanged_directory(
    db: &mut Connection,
    root: i64,
    parent: &Path,
    crawl: bool,
    new: &[(PathBuf, identity::Metadata)],
) -> anyhow::Result<()> {
    let txn = db.transaction()?;
    if crawl {
        for (path, metadata) in new {
            let descend = metadata.directory
                && !metadata.reparse
                && metadata.attributes & (0x1000 | 0x40000 | 0x400000) == 0;
            if descend {
                enqueue(&txn, root, path, true)?;
            }
        }
    }
    txn.execute(
        "DELETE FROM problems WHERE root=?1 AND path=?2",
        params![root, encode_path(parent)],
    )?;
    txn.execute(
        "DELETE FROM frontier WHERE root=?1 AND path=?2",
        params![root, encode_path(parent)],
    )?;
    txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    txn.commit()?;
    Ok(())
}

fn next_dir_entry(
    source: &mut DirSource,
    replay: &mut std::vec::IntoIter<(PathBuf, Option<identity::Metadata>)>,
) -> anyhow::Result<Option<(PathBuf, Option<identity::Metadata>)>> {
    if let Some(entry) = replay.next() {
        return Ok(Some(entry));
    }
    source.next()
}
fn artifact_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        ".git" | ".svn" | ".hg" | ".vs" | ".idea" | "__pycache__" | ".pytest_cache" | ".next"
        | ".nuxt" => true,
        "target" => path
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file()),
        "node_modules" => path.parent().is_some_and(|parent| {
            parent.join("package.json").is_file()
                || parent.join("pnpm-lock.yaml").is_file()
                || parent.join("yarn.lock").is_file()
        }),
        ".venv" | "venv" => path.parent().is_some_and(|parent| {
            parent.join("pyproject.toml").is_file()
                || parent.join("requirements.txt").is_file()
                || parent.join("Pipfile").is_file()
        }),
        _ => false,
    }
}

/// Excluded artifact directories are removed from the index when encountered;
/// they stay visible in the directory grid but do not contribute search rows.
fn purge_artifact_subtree(
    txn: &rusqlite::Transaction<'_>,
    root: i64,
    path: &Path,
) -> anyhow::Result<usize> {
    let encoded = encode_path(path);
    let prefix_length = encoded.len() as i64;
    let removed = txn.execute(
        "DELETE FROM entries WHERE root=?1 AND substr(path,1,?2)=?3",
        params![root, prefix_length, encoded],
    )?;
    txn.execute(
        "DELETE FROM frontier WHERE root=?1 AND substr(path,1,?2)=?3",
        params![root, prefix_length, encoded],
    )?;
    txn.execute(
        "DELETE FROM problems WHERE root=?1 AND substr(path,1,?2)=?3",
        params![root, prefix_length, encoded],
    )?;
    Ok(removed)
}

fn reconcile(
    db: &mut Connection,
    root: i64,
    parent: &Path,
    crawl: bool,
    excluded: &Path,
    stop: &AtomicBool,
    tx: &async_channel::Sender<crate::model::Event>,
) -> anyhow::Result<()> {
    db.execute("UPDATE meta SET value=value+1 WHERE key='epoch'", [])?;
    let epoch = db.query_row("SELECT value FROM meta WHERE key='epoch'", [], |r| {
        r.get::<_, i64>(0)
    })?;
    let mut source = match crate::index_bulk::DirectoryReader::open(parent) {
        Ok(reader) => DirSource::Bulk(reader),
        Err(_) => match std::fs::read_dir(parent) {
            Ok(entries) => DirSource::System(Box::new(entries)),
            Err(error) => {
                let txn = db.transaction()?;
                let removed = erase_missing(&txn, root, parent, epoch)?;
                txn.execute(
                    "UPDATE roots SET count=max(0,count-?2) WHERE id=?1",
                    params![root, removed as i64],
                )?;
                txn.execute(
                    "DELETE FROM frontier WHERE root=?1 AND path=?2",
                    params![root, encode_path(parent)],
                )?;
                if error.kind() != std::io::ErrorKind::NotFound
                    || txn.query_row("SELECT path FROM roots WHERE id=?1", [root], |r| {
                        r.get::<_, Vec<u8>>(0)
                    })? == encode_path(parent)
                {
                    txn.execute(
                        "INSERT OR REPLACE INTO problems VALUES(?1,?2,?3)",
                        params![root, encode_path(parent), error.to_string()],
                    )?;
                }
                txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
                txn.commit()?;
                return Ok(());
            }
        },
    };
    let mut replay: std::vec::IntoIter<(PathBuf, Option<identity::Metadata>)> =
        Vec::new().into_iter();
    // Keyed directory-level diff: if the bulk enumeration of a directory matches the
    // rows already indexed there, skip every per-entry write. Directories that
    // changed fall through to the epoch-based full upsert below.
    if matches!(&source, DirSource::Bulk(_))
        && let Some(old) = old_children(db, root, parent, FAST_PATH_LIMIT)?
        && !old.is_empty()
    {
        let mut new_entries: Vec<(PathBuf, identity::Metadata)> = Vec::new();
        let mut failed = false;
        loop {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            #[cfg(test)]
            let enumerate_start = Instant::now();
            match source.next() {
                Ok(Some((path, Some(metadata)))) => {
                    #[cfg(test)]
                    {
                        crate::index::profile::add(
                            &crate::index::profile::ENUMERATE_NS,
                            enumerate_start,
                        );
                        crate::index::profile::RECONCILED_ENTRIES.fetch_add(1, Ordering::Relaxed);
                    }
                    if metadata.directory && artifact_directory(&path) {
                        continue;
                    }
                    new_entries.push((path, metadata));
                    if new_entries.len() > FAST_PATH_LIMIT {
                        break;
                    }
                }
                Ok(Some((_, None))) => {
                    // Bulk enumeration always carries metadata; an unexpected shape
                    // falls back to the ordinary walk.
                    failed = true;
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            if let Ok(entries) = std::fs::read_dir(parent) {
                source = DirSource::System(Box::new(entries));
                replay = Vec::new().into_iter();
            }
        } else if new_entries.len() <= FAST_PATH_LIMIT
            && !new_entries.iter().any(|(_, metadata)| {
                metadata.reparse || metadata.attributes & (0x1000 | 0x40000 | 0x400000) != 0
            })
            && unchanged_directory(&old, &new_entries)
        {
            return finish_unchanged_directory(db, root, parent, crawl, &new_entries);
        } else {
            replay = new_entries
                .into_iter()
                .map(|(path, metadata)| (path, Some(metadata)))
                .collect::<Vec<_>>()
                .into_iter();
        }
    }
    let mut notice = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let txn = db.transaction()?;
        let mut finished = false;
        let mut bulk_failed = false;
        for _ in 0..128 {
            #[cfg(test)]
            let enumerate_start = Instant::now();
            let next = match next_dir_entry(&mut source, &mut replay) {
                Ok(next) => next,
                Err(_) => {
                    // A malformed/short-lived bulk reply falls back to the ordinary
                    // filesystem walk and restarts this directory; upserts are
                    // idempotent so already-written entries are harmless.
                    if !bulk_failed && matches!(source, DirSource::Bulk(_)) {
                        bulk_failed = true;
                        if let Ok(entries) = std::fs::read_dir(parent) {
                            source = DirSource::System(Box::new(entries));
                        } else {
                            finished = true;
                        }
                    }
                    continue;
                }
            };
            let Some((path, bulk_metadata)) = next else {
                finished = true;
                break;
            };
            #[cfg(test)]
            {
                crate::index::profile::add(&crate::index::profile::ENUMERATE_NS, enumerate_start);
                crate::index::profile::RECONCILED_ENTRIES.fetch_add(1, Ordering::Relaxed);
            }
            if path
                .components()
                .any(|c| crate::operations::archive::temporary_name(c.as_os_str()))
                || path.starts_with(excluded)
                || path.starts_with(std::env::temp_dir().join("Backleaf"))
            {
                continue;
            }
            let metadata = match bulk_metadata {
                Some(metadata) => metadata,
                None => {
                    #[cfg(test)]
                    let identity_start = Instant::now();
                    let captured = match identity::capture(&path) {
                        Ok(m) => m,
                        Err(error) => {
                            txn.execute(
                                "INSERT OR REPLACE INTO problems VALUES(?1,?2,?3)",
                                params![root, encode_path(&path), error.to_string()],
                            )?;
                            continue;
                        }
                    };
                    #[cfg(test)]
                    crate::index::profile::add(&crate::index::profile::IDENTITY_NS, identity_start);
                    captured
                }
            };
            if metadata.directory && artifact_directory(&path) {
                let removed = purge_artifact_subtree(&txn, root, &path)?;
                if removed > 0 {
                    txn.execute(
                        "UPDATE roots SET count=max(0,count-?2) WHERE id=?1",
                        params![root, removed as i64],
                    )?;
                }
                continue;
            }
            #[cfg(test)]
            let upsert_start = Instant::now();
            upsert(&txn, root, parent, &path, &metadata, epoch, crawl)?;
            #[cfg(test)]
            crate::index::profile::add(&crate::index::profile::UPSERT_NS, upsert_start);
        }
        if finished {
            let removed = erase_missing(&txn, root, parent, epoch)?;
            txn.execute(
                "UPDATE roots SET count=max(0,count-?2)WHERE id=?1",
                params![root, removed as i64],
            )?;
            txn.execute(
                "DELETE FROM problems WHERE root=?1 AND path=?2",
                params![root, encode_path(parent)],
            )?;
            txn.execute(
                "DELETE FROM frontier WHERE root=?1 AND path=?2",
                params![root, encode_path(parent)],
            )?;
        }
        txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        txn.commit()?;
        if notice.elapsed() >= Duration::from_secs(1) {
            let _ = tx.try_send(crate::model::Event::IndexStatus(status(db)?));
            notice = Instant::now();
        }
        if finished {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn status(db: &Connection) -> anyhow::Result<Status> {
    let revision = db.query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
        crate::operations::ledger::read_u64(r, 0)
    })?;
    let (count, building) = db.query_row(
        "SELECT coalesce(sum(count),0),coalesce(max(building),0) FROM roots",
        [],
        |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, bool>(1)?)),
    )?;
    let unavailable =
        db.query_row("SELECT count(*) FROM problems", [], |r| r.get::<_, i64>(0))? as usize;
    let mut q = db.prepare("SELECT path FROM roots ORDER BY id")?;
    let roots = q
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .map(|r| r.map(|p| decode_path(&p)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Status {
        revision,
        count,
        building,
        unavailable,
        roots,
        error: None,
    })
}
impl Service {
    pub fn send(&self, action: Action) -> Result<(), String> {
        self.1
            .try_send(action)
            .map_err(|_| "索引设置忙，请稍后重试".into())
    }
    pub fn new(
        directory: PathBuf,
        initial_roots: Option<Vec<PathBuf>>,
        tx: async_channel::Sender<crate::model::Event>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let halted = stop.clone();
        let (admin, commands) = mpsc::sync_channel(16);
        std::thread::Builder::new().name("backleaf-index".into()).spawn(move||{
            let result=(||->anyhow::Result<()> {
                crate::idle::enter_background_priority();
                let mut db=open(&directory)?;let _apartment=crate::operations::windows::Apartment::new()?;let directory=directory.canonicalize()?;let (changes,rx)=mpsc::sync_channel(128);
                let mut owner=None;let mut watches=std::collections::BTreeMap::<i64,(PathBuf,crate::index_watch::Watch)>::new();let mut last=Instant::now()-Duration::from_secs(2);let mut sent=String::new();let mut owner_attempt=Instant::now()-Duration::from_secs(2);let mut last_reconcile=Instant::now();
                while !halted.load(Ordering::Acquire)&&!tx.is_closed(){
                    let mut worked=false;
                    if owner.is_none() && owner_attempt.elapsed() >= Duration::from_secs(2) {
                        owner_attempt=Instant::now();
                        use std::os::windows::ffi::OsStrExt;use windows::{core::PCWSTR,Win32::Storage::FileSystem::*};
                        let lock=directory.join("index-writer.lock").as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
                        owner=unsafe{CreateFileW(PCWSTR(lock.as_ptr()),FILE_GENERIC_READ.0|FILE_GENERIC_WRITE.0,FILE_SHARE_MODE(0),None,OPEN_ALWAYS,FILE_ATTRIBUTE_NORMAL,None)}.ok().map(WriterLock);
                        if owner.is_some(){
                            let initialized=db.query_row("SELECT count(*) FROM meta WHERE key='initialized'",[],|r|r.get::<_,i64>(0))?;
                            if initialized==0{let roots=initial_roots.clone().unwrap_or_else(||crate::navigation::common_locations().into_iter().skip(1).map(|l|l.path).collect());for path in roots{let _=register(&db,&path);}db.execute("INSERT OR IGNORE INTO meta VALUES('initialized',1)",[])?;}
                            migrate_short_grams(&mut db)?;
                            let existing=status(&db)?.roots;for root in existing{let Some(id)=db.query_row("SELECT id FROM roots WHERE path=?1",[encode_path(&root)],|r|r.get::<_,i64>(0)).optional()? else{continue;};let actual=root.canonicalize().unwrap_or_else(|_|root.clone());let txn=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;if actual!=root{migrate_root_path(&txn,id,&root,&actual)?;}let stored=txn.query_row("SELECT path FROM roots WHERE id=?1",[id],|r|r.get::<_,Vec<u8>>(0)).optional()?;if stored.as_deref()==Some(encode_path(&actual).as_slice()){enqueue(&txn,id,&actual,true)?;}txn.commit()?;}
                        }
                    }
                    for _ in 0..16 {
                        let Ok(action)=commands.try_recv() else{break;};
                        worked=true;
                        let outcome=(||->anyhow::Result<()>{match action {
                            Action::Add(path)=>register(&db,&path)?,
                            Action::AddOpportunistic(path)=>{let _=register_opportunistic(&db,&path)?;},
                            Action::Touch(root)=>touch_root(&db,root)?,
                            Action::SetPins(pins)=>set_pins(&mut db,&pins)?,
                            Action::Remove(path)=>{db.execute("DELETE FROM roots WHERE path=?1",[encode_path(&path)])?;db.execute("DELETE FROM objects WHERE NOT EXISTS(SELECT 1 FROM entries WHERE entries.identity=objects.identity)",[])?;db.execute("UPDATE meta SET value=value+1 WHERE key='revision'",[])?;},
                            Action::Rescan=>{for path in status(&db)?.roots{let id=db.query_row("SELECT id FROM roots WHERE path=?1",[encode_path(&path)],|r|r.get(0))?;enqueue(&db,id,&path,true)?;}}
                        }Ok(())})();
                        if let Err(error)=outcome{let _=tx.send_blocking(crate::model::Event::IndexError(error.to_string()));}
                    }
                    let mut work_sleep=Duration::from_millis(1);
                    if owner.is_some(){
                        let mut q=db.prepare("SELECT id,path FROM roots ORDER BY id")?;let roots=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,decode_path(&r.get::<_,Vec<u8>>(1)?))))?.collect::<Result<Vec<_>,_>>()?;drop(q);
                        watches.retain(|id,(path,_)|roots.iter().any(|(root,current)|root==id&&current==path));
                        for (id,path) in roots{watches.entry(id).or_insert_with(||(path.clone(),crate::index_watch::Watch::new(id,path,directory.clone(),changes.clone())));}
                        // Process frontier directories in a time/count budget instead of
                        // one directory per loop; the fixed per-loop sleep used to add
                        // about 50 ms per directory to every build/rescan.
                        let mut processed=0usize;
                        let budget_start=Instant::now();
                        let budget=crate::idle::crawl_budget(crate::idle::system_idle());
                        work_sleep=budget.work_sleep;
                        loop {
                            // Keep the bounded notification mailbox responsive while
                            // directories are being processed.
                            let mut changes=Vec::with_capacity(128);
                            for _ in 0..128{match rx.try_recv(){Ok(change)=>changes.push(change),Err(_)=>break}}
                            if !changes.is_empty(){
                                let mut seen=std::collections::HashSet::with_capacity(changes.len());
                                changes.retain(|change| seen.insert((change.root,change.path.clone())));
                                incremental_batch(&mut db,&changes,&directory)?;
                                worked=true;
                            }
                            for (id,(_,watch)) in &watches{if watch.overflow.swap(false,Ordering::AcqRel)&&let Some(current)=db.query_row("SELECT path FROM roots WHERE id=?1",[id],|r|r.get::<_,Vec<u8>>(0)).optional()?{enqueue(&db,*id,&decode_path(&current),true)?;worked=true;}}
                            let next=db.query_row("SELECT root,path,crawl FROM frontier ORDER BY crawl,root,path LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,decode_path(&r.get::<_,Vec<u8>>(1)?),r.get::<_,bool>(2)?))).optional()?;
                            let Some((root,path,crawl))=next else{break;};
                            reconcile(&mut db,root,&path,crawl,&directory,&halted,&tx)?;
                            if !budget.directory_sleep.is_zero() {
                                std::thread::sleep(budget.directory_sleep);
                            }
                            last_reconcile=Instant::now();
                            processed+=1;
                            worked=true;
                            if processed>=budget.max_directories||budget_start.elapsed()>=budget.window||halted.load(Ordering::Acquire)||tx.is_closed(){break;}
                        }
                        db.execute("UPDATE roots SET building=EXISTS(SELECT 1 FROM frontier f WHERE f.root=roots.id) WHERE building<>EXISTS(SELECT 1 FROM frontier f WHERE f.root=roots.id)",[])?;
                        // Remote/unsupported notifications additionally reconcile after idle time.
                        if last_reconcile.elapsed()>Duration::from_secs(300){for(id,(path,_))in&watches{let text=path.to_string_lossy();if text.starts_with(r"\\?\UNC\"){enqueue(&db,*id,path,true)?;}}db.execute("DELETE FROM objects WHERE identity IN(SELECT identity FROM objects o WHERE NOT EXISTS(SELECT 1 FROM entries e WHERE e.identity=o.identity)LIMIT 4096)",[])?;let _=db.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");last_reconcile=Instant::now();}
                    }
                    if last.elapsed()>=Duration::from_secs(1){let value=status(&db)?;let stamp=format!("{}:{}:{}:{}",value.revision,value.count,value.building,value.unavailable);if stamp!=sent{tx.send_blocking(crate::model::Event::IndexStatus(value))?;sent=stamp;}last=Instant::now();}
                    // Yield only when there is no work; otherwise keep draining the
                    // frontier instead of paying a fixed idle delay per directory.
                    // While the user is active the idle policy lengthens the yield.
                    std::thread::sleep(if worked{work_sleep}else{Duration::from_millis(50)});
                }
                Ok(())
            })();
            if let Err(error)=result{let _=tx.send_blocking(crate::model::Event::IndexStatus(Status{error:Some(error.to_string()),..Default::default()}));}
        }).expect("metadata index service");
        Self(stop, admin)
    }
}
struct WriterLock(windows::Win32::Foundation::HANDLE);
#[cfg(test)]
pub(crate) fn build_test_index(root: &Path, state: &Path) -> Connection {
    let mut db = open(state).unwrap();
    register(&db, root).unwrap();
    let excluded = state.canonicalize().unwrap();
    for path in status(&db).unwrap().roots {
        let id = db
            .query_row(
                "SELECT id FROM roots WHERE path=?1",
                [encode_path(&path)],
                |r| r.get(0),
            )
            .unwrap();
        enqueue(&db, id, &path, true).unwrap();
    }
    let (tx, _rx) = async_channel::unbounded();
    while let Some((id, path, crawl)) = db
        .query_row("SELECT root,path,crawl FROM frontier LIMIT 1", [], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                decode_path(&r.get::<_, Vec<u8>>(1)?),
                r.get::<_, bool>(2)?,
            ))
        })
        .optional()
        .unwrap()
    {
        reconcile(
            &mut db,
            id,
            &path,
            crawl,
            &excluded,
            &AtomicBool::new(false),
            &tx,
        )
        .unwrap();
    }
    db.execute("UPDATE roots SET building=0", []).unwrap();
    db.execute("INSERT OR IGNORE INTO meta VALUES('grams_v2',1)", [])
        .unwrap();
    db
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artifact_directories_require_their_project_markers() {
        let f = crate::test_support::Fixture::new("index-artifacts");
        let rust = f.0.join("rust");
        std::fs::create_dir_all(rust.join("target")).unwrap();
        std::fs::write(rust.join("Cargo.toml"), b"[package]").unwrap();
        assert!(artifact_directory(&rust.join("target")));
        let plain = f.0.join("plain");
        std::fs::create_dir_all(plain.join("target")).unwrap();
        assert!(!artifact_directory(&plain.join("target")));
        let web = f.0.join("web");
        std::fs::create_dir_all(web.join("node_modules")).unwrap();
        std::fs::write(web.join("package.json"), b"{}").unwrap();
        assert!(artifact_directory(&web.join("node_modules")));
        assert!(artifact_directory(&f.0.join(".git")));
        assert!(!artifact_directory(&f.0.join("ordinary")));
    }
    #[test]
    fn index_excludes_generated_artifact_directories() {
        let f = crate::test_support::Fixture::new("index-artifact-build");
        let root = f.0.join("tree");
        std::fs::create_dir_all(root.join("target").join("debug")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(root.join("visible.txt"), b"data").unwrap();
        std::fs::write(
            root.join("target").join("debug").join("hidden.txt"),
            b"data",
        )
        .unwrap();
        std::fs::write(root.join(".git").join("hidden-git.txt"), b"data").unwrap();
        let root = root.canonicalize().unwrap();
        let state = f.0.join("state");
        let db = build_test_index(&root, &state);
        assert!(
            query_batch(&db, "hidden", 0, true, None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(query_batch(&db, "visible", 0, true, None).unwrap().len(), 1);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM entries WHERE name IN ('hidden.txt','hidden-git.txt')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn purge_artifact_subtree_removes_legacy_descendant_rows() {
        let f = crate::test_support::Fixture::new("index-artifact-purge");
        let root = f.0.join("tree");
        std::fs::create_dir_all(root.join("ordinary")).unwrap();
        std::fs::write(root.join("ordinary").join("keep.txt"), b"data").unwrap();
        let root = root.canonicalize().unwrap();
        let mut db = build_test_index(&root, &f.0.join("state"));
        let root_id: i64 = db
            .query_row("SELECT id FROM roots", [], |r| r.get(0))
            .unwrap();
        let target = root.join("target");
        let hidden = target.join("debug").join("hidden.txt");
        db.execute(
            "INSERT INTO entries(root,parent,path,name,display,directory,identity,epoch)
             VALUES(?1,?2,?3,'hidden.txt','hidden.txt',0,x'00',0)",
            params![root_id, encode_path(&target), encode_path(&hidden)],
        )
        .unwrap();
        let txn = db.transaction().unwrap();
        assert_eq!(purge_artifact_subtree(&txn, root_id, &target).unwrap(), 1);
        txn.commit().unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM entries", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn chinese_short_trigram_and_incremental_rename_delete_remain_consistent() {
        let f = crate::test_support::Fixture::new("index");
        let root = f.0.join("files");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let state = f.0.join("state");
        let mut db = open(&state).unwrap();
        register(&db, &root).unwrap();
        let root_id = db
            .query_row("SELECT id FROM roots", [], |r| r.get::<_, i64>(0))
            .unwrap();
        for name in [
            "文档报告.txt",
            "档案.docx",
            "README.md",
            "symbols\"literal.txt",
        ] {
            let path = root.join(name);
            if std::fs::write(&path, b"data").is_ok() {
                incremental(&mut db, root_id, &path, &state).unwrap();
            }
        }
        assert_eq!(query_batch(&db, "文", 0, true, None).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "文档", 0, true, None).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "档", 0, true, None).unwrap().len(), 2);
        assert_eq!(
            query_batch(&db, "报告.txt", 0, true, None).unwrap().len(),
            1
        );
        assert_eq!(query_batch(&db, "readme", 0, true, None).unwrap().len(), 1);
        db.execute(
            "INSERT INTO problems VALUES(?1,?2,'previously unavailable')",
            params![root_id, encode_path(&root.join("README.md"))],
        )
        .unwrap();
        incremental(&mut db, root_id, &root.join("README.md"), &state).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM problems", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        let old = root.join("文档报告.txt");
        let new = root.join("项目计划.txt");
        std::fs::rename(&old, &new).unwrap();
        incremental(&mut db, root_id, &old, &state).unwrap();
        incremental(&mut db, root_id, &new, &state).unwrap();
        assert!(query_batch(&db, "文档", 0, true, None).unwrap().is_empty());
        assert_eq!(query_batch(&db, "项目", 0, true, None).unwrap().len(), 1);
        std::fs::remove_file(&new).unwrap();
        incremental(&mut db, root_id, &new, &state).unwrap();
        assert!(query_batch(&db, "项目", 0, true, None).unwrap().is_empty());
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        db.execute("DELETE FROM roots WHERE id=?1", [root_id])
            .unwrap();
        incremental(&mut db, root_id, &root.join("README.md"), &state).unwrap();
        assert!(
            query_batch(&db, "readme", 0, true, None)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn scoped_plan_uses_identity_and_keeps_queries_inside_the_covering_root() {
        let f = crate::test_support::Fixture::new("index-scope");
        let root = f.0.join("files");
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let root = root.canonicalize().unwrap();
        let nested = nested.canonicalize().unwrap();
        for path in [root.join("scope.txt"), nested.join("scope-nested.txt")] {
            std::fs::write(&path, b"data").unwrap();
        }
        std::fs::create_dir(f.0.join("outside")).unwrap();
        std::fs::write(f.0.join("outside").join("scope-outside.txt"), b"data").unwrap();
        let state = f.0.join("state");
        let db = build_test_index(&root, &state);
        let recursive = plan_scope(&db, &root, true).unwrap().unwrap();
        assert_eq!(scope_size(&db, &recursive).unwrap(), 3);
        assert_eq!(estimate_matches(&db, "scope", &recursive, 16).unwrap(), 2);
        assert_eq!(estimate_matches(&db, "nested", &recursive, 16).unwrap(), 2);
        assert_eq!(estimate_matches(&db, "absent", &recursive, 16).unwrap(), 0);
        assert_eq!(
            query_batch(&db, "scope", 0, true, Some(&recursive))
                .unwrap()
                .len(),
            2
        );
        let flat = plan_scope(&db, &root, false).unwrap().unwrap();
        assert_eq!(scope_size(&db, &flat).unwrap(), 2);
        assert_eq!(
            query_batch(&db, "scope", 0, true, Some(&flat))
                .unwrap()
                .len(),
            1
        );
        let from_nested = plan_scope(&db, &nested, true).unwrap().unwrap();
        assert_eq!(
            query_batch(&db, "scope", 0, true, Some(&from_nested))
                .unwrap()
                .len(),
            1
        );
        assert!(plan_scope(&db, &f.0, true).unwrap().is_none());
        db.execute("UPDATE roots SET building=1", []).unwrap();
        assert!(plan_scope(&db, &root, true).unwrap().is_none());
        db.execute("UPDATE roots SET building=0", []).unwrap();
        let root_id: i64 = db
            .query_row("SELECT id FROM roots", [], |r| r.get(0))
            .unwrap();
        db.execute(
            "INSERT INTO problems VALUES(?1,?2,'unavailable')",
            params![root_id, encode_path(&root)],
        )
        .unwrap();
        assert!(plan_scope(&db, &root, true).unwrap().is_none());
    }

    #[test]
    fn unchanged_directory_diff_skips_entry_rewrites_and_tracks_changes() {
        let f = crate::test_support::Fixture::new("index-keyed-diff");
        let root = f.0.join("tree");
        std::fs::create_dir(&root).unwrap();
        for dir in 0..3 {
            let path = root.join(format!("d{dir}"));
            std::fs::create_dir(&path).unwrap();
            for file in 0..100 {
                std::fs::write(path.join(format!("file-{file:03}.txt")), b"data").unwrap();
            }
        }
        let root = root.canonicalize().unwrap();
        let state = f.0.join("state");
        let mut db = build_test_index(&root, &state);
        let root_id: i64 = db
            .query_row("SELECT id FROM roots", [], |r| r.get(0))
            .unwrap();
        let entry_count = |db: &Connection| -> i64 {
            db.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(entry_count(&db), 303);

        fn rescan(db: &mut Connection, root_id: i64, root: &Path, state: &Path) {
            enqueue(db, root_id, root, true).unwrap();
            let excluded = state.canonicalize().unwrap();
            let (tx, _rx) = async_channel::unbounded();
            let stop = AtomicBool::new(false);
            loop {
                let next = db
                    .query_row("SELECT root,path,crawl FROM frontier LIMIT 1", [], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            decode_path(&r.get::<_, Vec<u8>>(1)?),
                            r.get::<_, bool>(2)?,
                        ))
                    })
                    .optional()
                    .unwrap();
                let Some((id, path, crawl)) = next else {
                    break;
                };
                reconcile(db, id, &path, crawl, &excluded, &stop, &tx).unwrap();
            }
        }

        let writes_before = db.total_changes();
        rescan(&mut db, root_id, &root, &state);
        let unchanged_writes = db.total_changes() - writes_before;
        assert!(
            unchanged_writes < 64,
            "unchanged rescan wrote {unchanged_writes} rows"
        );
        assert_eq!(entry_count(&db), 303);

        std::fs::write(root.join("d1").join("file-000.txt"), b"changed content").unwrap();
        std::fs::remove_file(root.join("d2").join("file-001.txt")).unwrap();
        std::fs::write(root.join("d0").join("added.txt"), b"new").unwrap();
        rescan(&mut db, root_id, &root, &state);
        assert_eq!(entry_count(&db), 303);
        let absent = |path: PathBuf| -> i64 {
            db.query_row(
                "SELECT count(*) FROM entries WHERE path=?1",
                [encode_path(&path)],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(absent(root.join("d2").join("file-001.txt")), 0);
        assert_eq!(absent(root.join("d0").join("added.txt")), 1);
        let bytes: Option<i64> = db
            .query_row(
                "SELECT o.bytes FROM entries e JOIN objects o ON o.identity=e.identity WHERE e.path=?1",
                [encode_path(&root.join("d1").join("file-000.txt"))],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bytes, Some(b"changed content".len() as i64));
    }

    #[test]
    fn batched_incremental_applies_rename_and_create_in_one_transaction() {
        let f = crate::test_support::Fixture::new("index-batch");
        let root = f.0.join("tree");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let state = f.0.join("state");
        let mut db = open(&state).unwrap();
        register(&db, &root).unwrap();
        let root_id: i64 = db
            .query_row("SELECT id FROM roots", [], |r| r.get(0))
            .unwrap();
        let old = root.join("old.txt");
        std::fs::write(&old, b"old").unwrap();
        incremental(&mut db, root_id, &old, &state).unwrap();
        let renamed = root.join("renamed.txt");
        std::fs::rename(&old, &renamed).unwrap();
        let added = root.join("added.txt");
        std::fs::write(&added, b"added").unwrap();
        incremental_batch(
            &mut db,
            &[
                crate::index_watch::Change {
                    root: root_id,
                    path: renamed.clone(),
                    from: Some(old.clone()),
                },
                crate::index_watch::Change {
                    root: root_id,
                    path: added.clone(),
                    from: None,
                },
            ],
            &state,
        )
        .unwrap();
        let count = |path: &Path| -> i64 {
            db.query_row(
                "SELECT count(*) FROM entries WHERE path=?1",
                [encode_path(path)],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(count(&old), 0);
        assert_eq!(count(&renamed), 1);
        assert_eq!(count(&added), 1);
        assert_eq!(query_batch(&db, "renamed", 0, true, None).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "added", 0, true, None).unwrap().len(), 1);
    }

    #[test]
    fn short_ascii_queries_use_extended_grams_and_migrate_legacy_rows() {
        let f = crate::test_support::Fixture::new("index-short-grams");
        let root = f.0.join("files");
        std::fs::create_dir(&root).unwrap();
        for name in ["alpha-one.txt", "alpha-two.txt", "beta.txt", "报告.txt"] {
            std::fs::write(root.join(name), b"data").unwrap();
        }
        let root = root.canonicalize().unwrap();
        let state = f.0.join("state");
        let mut db = build_test_index(&root, &state);
        assert!(grams_ready(&db));
        let query =
            |db: &Connection, value: &str| query_batch(db, value, 0, true, None).unwrap().len();
        assert_eq!(query(&db, "al"), 2);
        assert_eq!(query(&db, "ph"), 2);
        assert_eq!(query(&db, "a"), 3);
        assert_eq!(query(&db, "报"), 1);

        // Simulate a database built by the CJK-only grams version: the reader must
        // stay correct through the instr fallback, then the one-time backfill
        // restores the short-query index.
        db.execute("DELETE FROM grams", []).unwrap();
        db.execute("DELETE FROM meta WHERE key='grams_v2'", [])
            .unwrap();
        assert!(!grams_ready(&db));
        assert_eq!(query(&db, "al"), 2);
        migrate_short_grams(&mut db).unwrap();
        assert!(grams_ready(&db));
        assert_eq!(query(&db, "al"), 2);
        assert_eq!(query(&db, "a"), 3);
    }

    #[test]
    fn opportunistic_roots_evict_lru_but_keep_pinned_roots() {
        let f = crate::test_support::Fixture::new("index-lru");
        let state = f.0.join("state");
        let mut db = open(&state).unwrap();
        let mut roots = Vec::new();
        for i in 0..7 {
            let path = f.0.join(format!("root-{i}"));
            std::fs::create_dir(&path).unwrap();
            let id = register_with_kind(&db, &path, 1)
                .unwrap()
                .expect("opportunistic root");
            roots.push((id, path));
        }
        // Pin one root, then make it the most recently used; all other roots keep
        // used=50 so the lowest id among them is the LRU victim.
        set_pins(&mut db, &[roots[0].1.clone()]).unwrap();
        for (id, _) in &roots {
            db.execute(
                "UPDATE roots SET used=?2 WHERE id=?1",
                params![id, if *id == roots[0].0 { 100 } else { 50 }],
            )
            .unwrap();
        }
        let eighth = f.0.join("root-7");
        std::fs::create_dir(&eighth).unwrap();
        register_with_kind(&db, &eighth, 1)
            .unwrap()
            .expect("8th root");
        let extra = f.0.join("root-extra");
        std::fs::create_dir(&extra).unwrap();
        register_with_kind(&db, &extra, 1)
            .unwrap()
            .expect("new root");
        let root_paths: Vec<PathBuf> = root_rows(&db)
            .unwrap()
            .into_iter()
            .map(|(_, path, _)| path)
            .collect();
        assert_eq!(root_paths.len(), MAX_OPPORTUNISTIC_ROOTS);
        let exists = |id: i64| -> i64 {
            db.query_row("SELECT count(*) FROM roots WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap()
        };
        assert_eq!(exists(roots[0].0), 1, "pinned root was evicted");
        assert_eq!(
            exists(roots[1].0),
            0,
            "least-recently-used unpinned root survived"
        );
        // Drive roots and temp trees are never acquired on demand.
        assert!(!opportunistic_path_allowed(&eighth.canonicalize().unwrap()));
        let before = root_rows(&db).unwrap().len();
        assert!(
            register_opportunistic(&db, Path::new("C:\\"))
                .unwrap()
                .is_none()
        );
        assert_eq!(root_rows(&db).unwrap().len(), before);
    }

    #[test]
    fn old_database_gains_lru_columns_and_pin_table() {
        let f = crate::test_support::Fixture::new("index-lru-migration");
        let state = f.0.join("state");
        std::fs::create_dir_all(&state).unwrap();
        {
            let legacy = Connection::open(database(&state)).unwrap();
            legacy
                .execute_batch(
                    "CREATE TABLE roots(id INTEGER PRIMARY KEY,path BLOB UNIQUE NOT NULL,count INTEGER NOT NULL DEFAULT 0,building INTEGER NOT NULL DEFAULT 1);",
                )
                .unwrap();
        }
        let db = open(&state).unwrap();
        let columns = db
            .prepare("PRAGMA table_info(roots)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "kind"));
        assert!(columns.iter().any(|column| column == "used"));
        let pins: i64 = db
            .query_row("SELECT count(*) FROM pins", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pins, 0);
    }
}
