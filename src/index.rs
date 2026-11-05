//! Persistent, ordinary-permission metadata index. No USN privileges or content upload.
use crate::operations::{
    ledger::{decode_path, encode_path},
    windows::identity,
};
use rusqlite::{Connection, OptionalExtension, params};
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
    Ok(db)
}
pub fn register(db: &Connection, path: &Path) -> anyhow::Result<()> {
    let path = path.canonicalize()?;
    anyhow::ensure!(path.is_dir(), "索引位置不是文件夹");
    let mut q = db.prepare("SELECT path FROM roots")?;
    let existing = q
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(q);
    if existing.iter().any(|raw| {
        let registered = decode_path(raw);
        path.starts_with(registered.canonicalize().unwrap_or(registered))
    }) {
        return Ok(());
    }
    anyhow::ensure!(existing.len() < 64, "索引位置最多64个");
    let raw = encode_path(&path);
    db.execute("INSERT OR IGNORE INTO roots(path)VALUES(?1)", [&raw])?;
    let root = db.query_row("SELECT id FROM roots WHERE path=?1", [&raw], |r| {
        r.get::<_, i64>(0)
    })?;
    enqueue(db, root, &path, true)?;
    Ok(())
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
pub fn short_grams(name: &str) -> std::collections::BTreeSet<String> {
    let chars = name.chars().collect::<Vec<_>>();
    let mut set = std::collections::BTreeSet::new();
    for (i, &ch) in chars.iter().enumerate() {
        if cjk(ch) {
            set.insert(ch.to_string());
            if let Some(&next) = chars.get(i + 1).filter(|c| cjk(**c)) {
                set.insert(format!("{ch}{next}"));
            }
        }
    }
    set
}
pub struct IndexedItem {
    pub id: i64,
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub identity: Vec<u8>,
}
pub fn query_batch(
    db: &Connection,
    query: &str,
    after: i64,
    recursive: bool,
) -> anyhow::Result<Vec<IndexedItem>> {
    let query = query.to_lowercase();
    let n = query.chars().count();
    let (from, predicate, value) = if (1..=2).contains(&n) && query.chars().all(cjk) {
        (
            "grams g JOIN entries e ON e.id=g.id",
            "g.gram=?1 AND g.id>?2",
            query,
        )
    } else if n >= 3 {
        (
            "names JOIN entries e ON e.id=names.rowid",
            "names MATCH ?1 AND names.rowid>?2",
            format!("\"{}\"", query.replace('"', "\"\"")),
        )
    } else {
        ("entries e", "instr(e.name,?1)>0 AND e.id>?2", query)
    };
    let mut q=db.prepare(&format!("SELECT e.id,e.display,e.path,e.directory,e.identity FROM {from} WHERE {predicate} AND (?3 OR e.parent IN(SELECT path FROM roots)) ORDER BY e.id LIMIT 256"))?;
    let rows = q.query_map(params![value, after, recursive], |r| {
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
        for gram in short_grams(&name) {
            db.execute("INSERT INTO grams VALUES(?1,?2)", params![gram, id])?;
        }
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
pub(crate) fn incremental(
    db: &mut Connection,
    root: i64,
    path: &Path,
    excluded: &Path,
) -> anyhow::Result<()> {
    if path.starts_with(excluded)
        || path
            .components()
            .any(|c| crate::operations::archive::temporary_name(c.as_os_str()))
    {
        return Ok(());
    }
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
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
    if actual_root != registered_path {
        migrate_root_path(&txn, root, &registered_path, &actual_root)?;
        txn.commit()?;
        return Ok(());
    }
    match identity::capture(path) {
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
                erase_path(&txn, root, path)?;
            }
            if normalized.starts_with(decode_path(&registered))
                && let Some(parent) = normalized.parent()
            {
                upsert(&txn, root, parent, &normalized, &metadata, epoch, false)?;
                if metadata.reparse || metadata.attributes & (0x1000 | 0x40000 | 0x400000) != 0 {
                    enqueue(&txn, root, parent, false)?;
                }
            }
        }
        Err(error) => {
            erase_path(&txn, root, path)?;
            if path.exists() {
                txn.execute(
                    "INSERT OR REPLACE INTO problems VALUES(?1,?2,?3)",
                    params![root, encode_path(path), error.to_string()],
                )?;
            }
        }
    }
    txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    txn.commit()?;
    Ok(())
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
    let read = std::fs::read_dir(parent);
    let mut entries = match read {
        Ok(r) => r,
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
    };
    let mut notice = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let txn = db.transaction()?;
        let mut finished = false;
        for _ in 0..128 {
            let Some(entry) = entries.next() else {
                finished = true;
                break;
            };
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if path
                .components()
                .any(|c| crate::operations::archive::temporary_name(c.as_os_str()))
                || path.starts_with(excluded)
                || path.starts_with(std::env::temp_dir().join("Backleaf"))
            {
                continue;
            }
            let metadata = match identity::capture(&path) {
                Ok(m) => m,
                Err(error) => {
                    txn.execute(
                        "INSERT OR REPLACE INTO problems VALUES(?1,?2,?3)",
                        params![root, encode_path(&path), error.to_string()],
                    )?;
                    continue;
                }
            };
            upsert(&txn, root, parent, &path, &metadata, epoch, crawl)?;
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
                let mut db=open(&directory)?;let _apartment=crate::operations::windows::Apartment::new()?;let directory=directory.canonicalize()?;let (changes,rx)=mpsc::sync_channel(128);
                let mut owner=None;let mut watches=std::collections::BTreeMap::<i64,(PathBuf,crate::index_watch::Watch)>::new();let mut last=Instant::now()-Duration::from_secs(2);let mut sent=String::new();let mut owner_attempt=Instant::now()-Duration::from_secs(2);let mut last_reconcile=Instant::now();
                while !halted.load(Ordering::Acquire)&&!tx.is_closed(){
                    if owner.is_none() && owner_attempt.elapsed() >= Duration::from_secs(2) {
                        owner_attempt=Instant::now();
                        use std::os::windows::ffi::OsStrExt;use windows::{core::PCWSTR,Win32::Storage::FileSystem::*};
                        let lock=directory.join("index-writer.lock").as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
                        owner=unsafe{CreateFileW(PCWSTR(lock.as_ptr()),FILE_GENERIC_READ.0|FILE_GENERIC_WRITE.0,FILE_SHARE_MODE(0),None,OPEN_ALWAYS,FILE_ATTRIBUTE_NORMAL,None)}.ok().map(WriterLock);
                        if owner.is_some(){
                            let initialized=db.query_row("SELECT count(*) FROM meta WHERE key='initialized'",[],|r|r.get::<_,i64>(0))?;
                            if initialized==0{let roots=initial_roots.clone().unwrap_or_else(||crate::navigation::common_locations().into_iter().skip(1).map(|l|l.path).collect());for path in roots{let _=register(&db,&path);}db.execute("INSERT OR IGNORE INTO meta VALUES('initialized',1)",[])?;}
                            let existing=status(&db)?.roots;for root in existing{let Some(id)=db.query_row("SELECT id FROM roots WHERE path=?1",[encode_path(&root)],|r|r.get::<_,i64>(0)).optional()? else{continue;};let actual=root.canonicalize().unwrap_or_else(|_|root.clone());let txn=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;if actual!=root{migrate_root_path(&txn,id,&root,&actual)?;}let stored=txn.query_row("SELECT path FROM roots WHERE id=?1",[id],|r|r.get::<_,Vec<u8>>(0)).optional()?;if stored.as_deref()==Some(encode_path(&actual).as_slice()){enqueue(&txn,id,&actual,true)?;}txn.commit()?;}
                        }
                    }
                    for _ in 0..16 {
                        let Ok(action)=commands.try_recv() else{break;};
                        let outcome=(||->anyhow::Result<()>{match action {
                            Action::Add(path)=>register(&db,&path)?,
                            Action::Remove(path)=>{db.execute("DELETE FROM roots WHERE path=?1",[encode_path(&path)])?;db.execute("DELETE FROM objects WHERE NOT EXISTS(SELECT 1 FROM entries WHERE entries.identity=objects.identity)",[])?;db.execute("UPDATE meta SET value=value+1 WHERE key='revision'",[])?;},
                            Action::Rescan=>{for path in status(&db)?.roots{let id=db.query_row("SELECT id FROM roots WHERE path=?1",[encode_path(&path)],|r|r.get(0))?;enqueue(&db,id,&path,true)?;}}
                        }Ok(())})();
                        if let Err(error)=outcome{let _=tx.send_blocking(crate::model::Event::IndexError(error.to_string()));}
                    }
                    if owner.is_some(){
                        let mut q=db.prepare("SELECT id,path FROM roots ORDER BY id")?;let roots=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,decode_path(&r.get::<_,Vec<u8>>(1)?))))?.collect::<Result<Vec<_>,_>>()?;drop(q);
                        watches.retain(|id,(path,_)|roots.iter().any(|(root,current)|root==id&&current==path));
                        for (id,path) in roots{watches.entry(id).or_insert_with(||(path.clone(),crate::index_watch::Watch::new(id,path,directory.clone(),changes.clone())));}
                        for _ in 0..128{match rx.try_recv(){Ok(change)=>{incremental(&mut db,change.root,&change.path,&directory)?;},Err(_)=>break}}
                        for (id,(_,watch)) in &watches{if watch.overflow.swap(false,Ordering::AcqRel)&&let Some(current)=db.query_row("SELECT path FROM roots WHERE id=?1",[id],|r|r.get::<_,Vec<u8>>(0)).optional()?{enqueue(&db,*id,&decode_path(&current),true)?;}}
                        let next=db.query_row("SELECT root,path,crawl FROM frontier ORDER BY crawl,root,path LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,decode_path(&r.get::<_,Vec<u8>>(1)?),r.get::<_,bool>(2)?))).optional()?;
                        if let Some((root,path,crawl))=next{reconcile(&mut db,root,&path,crawl,&directory,&halted,&tx)?;last_reconcile=Instant::now();}
                        db.execute("UPDATE roots SET building=EXISTS(SELECT 1 FROM frontier f WHERE f.root=roots.id) WHERE building<>EXISTS(SELECT 1 FROM frontier f WHERE f.root=roots.id)",[])?;
                        // Remote/unsupported notifications additionally reconcile after idle time.
                        if last_reconcile.elapsed()>Duration::from_secs(300){for(id,(path,_))in&watches{let text=path.to_string_lossy();if text.starts_with(r"\\?\UNC\"){enqueue(&db,*id,path,true)?;}}db.execute("DELETE FROM objects WHERE identity IN(SELECT identity FROM objects o WHERE NOT EXISTS(SELECT 1 FROM entries e WHERE e.identity=o.identity)LIMIT 4096)",[])?;let _=db.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");last_reconcile=Instant::now();}
                    }
                    if last.elapsed()>=Duration::from_secs(1){let value=status(&db)?;let stamp=format!("{}:{}:{}:{}",value.revision,value.count,value.building,value.unavailable);if stamp!=sent{tx.send_blocking(crate::model::Event::IndexStatus(value))?;sent=stamp;}last=Instant::now();}
                    std::thread::sleep(Duration::from_millis(50));
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
        assert_eq!(query_batch(&db, "文", 0, true).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "文档", 0, true).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "档", 0, true).unwrap().len(), 2);
        assert_eq!(query_batch(&db, "报告.txt", 0, true).unwrap().len(), 1);
        assert_eq!(query_batch(&db, "readme", 0, true).unwrap().len(), 1);
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
        assert!(query_batch(&db, "文档", 0, true).unwrap().is_empty());
        assert_eq!(query_batch(&db, "项目", 0, true).unwrap().len(), 1);
        std::fs::remove_file(&new).unwrap();
        incremental(&mut db, root_id, &new, &state).unwrap();
        assert!(query_batch(&db, "项目", 0, true).unwrap().is_empty());
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        db.execute("DELETE FROM roots WHERE id=?1", [root_id])
            .unwrap();
        incremental(&mut db, root_id, &root.join("README.md"), &state).unwrap();
        assert!(query_batch(&db, "readme", 0, true).unwrap().is_empty());
    }
}
