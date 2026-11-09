//! SQLite transactions and revision checks coordinate independent windows/processes.
use crate::{
    file_view::{FileView, WinPath},
    model::Event,
};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender, TrySendError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    Folder(WinPath),
    View(FileView),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub destination: Destination,
    pub revision: u64,
}
pub enum Action {
    Seed(Vec<(String, PathBuf)>),
    Save {
        token: u64,
        id: Option<String>,
        revision: u64,
        title: String,
        view: FileView,
    },
    Remove {
        id: String,
        revision: u64,
    },
    Pin {
        path: PathBuf,
        title: String,
        before: Option<String>,
        revision: u64,
    },
    Reorder {
        ids: Vec<String>,
        revision: u64,
    },
}
pub enum Notice {
    Loaded {
        revision: u64,
        entries: Vec<Entry>,
    },
    Saved {
        token: u64,
        result: Result<Entry, String>,
    },
    Error(String),
}
pub struct Store(SyncSender<Action>);
impl Store {
    pub fn new(directory: PathBuf, tx: async_channel::Sender<Event>) -> Self {
        let (sender, rx) = mpsc::sync_channel(16);
        std::thread::Builder::new()
            .name("backleaf-quick-access".into())
            .spawn(move || {
                let mut db = None;
                let mut sent = u64::MAX;
                let mut last_error = None;
                let mut seed = None;
                loop {
                    let action = match rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(a) => Some(a),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => return,
                    };
                    if tx.is_closed() {
                        return;
                    }
                    let mut opening_error = None;
                    if db.is_none() {
                        match open(&directory) {
                            Ok(connection) => {
                                db = Some(connection);
                                sent = u64::MAX;
                                last_error = None;
                            }
                            Err(error) => opening_error = Some(error.to_string()),
                        }
                    }
                    if let Some(action) = action {
                        let token = if let Action::Save { token, .. } = &action {
                            Some(*token)
                        } else {
                            None
                        };
                        if let Action::Seed(rows) = action {
                            seed = Some(rows);
                        } else {
                            let outcome = if let Some(connection) = &mut db {
                                apply(connection, action).map_err(|e| e.to_string())
                            } else {
                                Err(opening_error
                                    .clone()
                                    .unwrap_or_else(|| "导航设置不可用".into()))
                            };
                            match (token, outcome) {
                                (Some(token), outcome) => {
                                    let result = outcome
                                        .and_then(|entry| entry.ok_or_else(|| "未保存视图".into()));
                                    let _ = tx.send_blocking(Event::QuickAccess(Notice::Saved {
                                        token,
                                        result,
                                    }));
                                }
                                (None, Err(error)) => {
                                    let _ =
                                        tx.send_blocking(Event::QuickAccess(Notice::Error(error)));
                                }
                                _ => {}
                            }
                        }
                    }
                    let outcome = (|| -> anyhow::Result<()> {
                        let Some(connection) = &mut db else {
                            return Err(anyhow::anyhow!(
                                opening_error.unwrap_or_else(|| "导航设置不可用".into())
                            ));
                        };
                        if let Some(rows) = &seed {
                            apply(connection, Action::Seed(rows.clone()))?;
                            seed = None;
                        }
                        let revision = connection.query_row(
                            "SELECT value FROM meta WHERE key='revision'",
                            [],
                            |r| crate::operations::ledger::read_u64(r, 0),
                        )?;
                        if revision != sent {
                            let entries = load(connection)?;
                            tx.send_blocking(Event::QuickAccess(Notice::Loaded {
                                revision,
                                entries,
                            }))?;
                            sent = revision;
                        }
                        Ok(())
                    })();
                    if let Err(error) = outcome {
                        let error = error.to_string();
                        db = None;
                        if last_error.as_ref() != Some(&error) {
                            let _ =
                                tx.send_blocking(Event::QuickAccess(Notice::Error(error.clone())));
                            last_error = Some(error);
                        }
                    }
                }
            })
            .expect("quick access store");
        Self(sender)
    }
    pub fn send(&self, action: Action) -> Result<(), String> {
        self.0.try_send(action).map_err(|e| {
            match e {
                TrySendError::Full(_) => "正在保存，请稍后重试",
                TrySendError::Disconnected(_) => "导航设置不可用",
            }
            .into()
        })
    }
}
fn open(directory: &std::path::Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(directory)?;
    let db = Connection::open(directory.join("quick-access.sqlite"))?;
    db.busy_timeout(Duration::from_secs(2))?;
    db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL); INSERT OR IGNORE INTO meta VALUES('revision',0); CREATE TABLE IF NOT EXISTS entries(id TEXT PRIMARY KEY,title TEXT NOT NULL,payload TEXT NOT NULL,revision INTEGER NOT NULL,position INTEGER NOT NULL);")?;
    Ok(db)
}
/// Read-only snapshot for non-UI consumers (taskbar jump list, `--view`
/// startup). WAL readers do not block the UI store writer.
pub fn read_entries(directory: &Path) -> Vec<Entry> {
    let debug = std::env::var_os("BACKLEAF_JUMP_LIST_DUMP").is_some();
    let connection = match read_connection(directory) {
        Ok(connection) => connection,
        Err(error) => {
            if debug {
                eprintln!("quick-access open: {error}");
            }
            return Vec::new();
        }
    };
    let mut query = match connection
        .prepare("SELECT id,title,payload,revision FROM entries ORDER BY position,id LIMIT 513")
    {
        Ok(query) => query,
        Err(error) => {
            if debug {
                eprintln!("quick-access query: {error}");
            }
            return Vec::new();
        }
    };
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            crate::operations::ledger::read_u64(row, 3)?,
        ))
    });
    let Ok(rows) = rows else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for row in rows {
        match row {
            Ok((id, title, payload, revision)) => match serde_json::from_str(&payload) {
                Ok(destination) => entries.push(Entry {
                    id,
                    title,
                    destination,
                    revision,
                }),
                Err(error) => {
                    if debug {
                        eprintln!("quick-access payload {id}: {error}");
                    }
                }
            },
            Err(error) => {
                if debug {
                    eprintln!("quick-access row: {error}");
                }
            }
        }
    }
    entries.truncate(512);
    entries
}
/// Read one saved entry by id; used by jump-list view links.
pub fn read_entry(directory: &Path, id: &str) -> Option<Entry> {
    let connection = read_connection(directory).ok()?;
    connection
        .query_row(
            "SELECT id,title,payload,revision FROM entries WHERE id=?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    crate::operations::ledger::read_u64(row, 3)?,
                ))
            },
        )
        .ok()
        .and_then(|(id, title, payload, revision)| {
            let destination = serde_json::from_str(&payload).ok()?;
            Some(Entry {
                id,
                title,
                destination,
                revision,
            })
        })
}
fn read_connection(directory: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        directory.join("quick-access.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let _ = connection.busy_timeout(Duration::from_secs(2));
    Ok(connection)
}
fn load(db: &Connection) -> anyhow::Result<Vec<Entry>> {
    let mut q =
        db.prepare("SELECT id,title,payload,revision FROM entries ORDER BY position,id LIMIT 513")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            crate::operations::ledger::read_u64(r, 3)?,
        ))
    })?;
    let mut entries = Vec::new();
    for row in rows {
        let (id, title, payload, revision) = row?;
        entries.push(Entry {
            id,
            title,
            destination: serde_json::from_str(&payload)?,
            revision,
        });
    }
    anyhow::ensure!(entries.len() <= 512, "导航项目超过上限");
    Ok(entries)
}
fn apply(db: &mut Connection, action: Action) -> anyhow::Result<Option<Entry>> {
    let txn = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let current = txn.query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
        crate::operations::ledger::read_u64(r, 0)
    })?;
    let mut entries = load(&txn)?;
    let mut saved = None;
    let mut changed = true;
    match action {
        Action::Seed(rows) => {
            let initialized = txn.query_row(
                "SELECT count(*) FROM meta WHERE key='initialized'",
                [],
                |r| crate::operations::ledger::read_u64(r, 0),
            )?;
            if initialized > 0 {
                changed = false;
            } else {
                for (i, (title, path)) in rows.into_iter().enumerate() {
                    entries.push(Entry {
                        id: format!("system-{i}"),
                        title,
                        destination: Destination::Folder(WinPath::new(&path)),
                        revision: 1,
                    });
                }
                txn.execute("INSERT INTO meta VALUES('initialized',1)", [])?;
            }
        }
        Action::Save {
            id,
            revision,
            title,
            view,
            ..
        } => {
            view.validate().map_err(anyhow::Error::msg)?;
            let title = title.trim().to_owned();
            anyhow::ensure!(
                !title.is_empty() && title.chars().count() <= 80,
                "名称需要1–80个字符"
            );
            let entry = if let Some(id) = id {
                let e = entries
                    .iter_mut()
                    .find(|e| e.id == id)
                    .ok_or_else(|| anyhow::anyhow!("此视图已被移除"))?;
                anyhow::ensure!(
                    e.revision == revision,
                    "此视图已在另一个窗口修改，请另存为或重新打开"
                );
                e.title = title;
                e.destination = Destination::View(view);
                e.revision += 1;
                e.clone()
            } else {
                let e = Entry {
                    id: format!(
                        "view-{}-{}",
                        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
                        std::process::id()
                    ),
                    title,
                    destination: Destination::View(view),
                    revision: 1,
                };
                entries.push(e.clone());
                e
            };
            saved = Some(entry);
        }
        Action::Remove { id, revision } => {
            let e = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("此入口已被移除"))?;
            anyhow::ensure!(e.revision == revision, "此入口已在另一个窗口修改");
            entries.retain(|e| e.id != id);
        }
        Action::Pin {
            path,
            title,
            before,
            revision,
        } => {
            anyhow::ensure!(current == revision, "导航顺序已改变，请重试");
            let canonical = path.canonicalize()?;
            anyhow::ensure!(canonical.is_dir(), "只能固定文件夹");
            let existing=entries.iter().position(|e|matches!(&e.destination,Destination::Folder(p)if p.path().canonicalize().ok().as_ref()==Some(&canonical)));
            let e = if let Some(i) = existing {
                entries.remove(i)
            } else {
                Entry {
                    id: format!(
                        "folder-{}-{}",
                        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
                        std::process::id()
                    ),
                    title,
                    destination: Destination::Folder(WinPath::new(&path)),
                    revision: 1,
                }
            };
            let index = before
                .and_then(|id| entries.iter().position(|e| e.id == id))
                .unwrap_or(entries.len());
            entries.insert(index, e);
        }
        Action::Reorder { ids, revision } => {
            anyhow::ensure!(current == revision, "导航顺序已改变，请重试");
            anyhow::ensure!(ids.len() == entries.len(), "导航列表已改变");
            let mut reordered = Vec::with_capacity(entries.len());
            for id in ids {
                let index = entries
                    .iter()
                    .position(|e| e.id == id)
                    .ok_or_else(|| anyhow::anyhow!("导航列表已改变"))?;
                reordered.push(entries.remove(index));
            }
            entries = reordered;
        }
    }
    anyhow::ensure!(entries.len() <= 512, "最多保存512个导航入口");
    if changed {
        txn.execute("DELETE FROM entries", [])?;
        for (position, e) in entries.into_iter().enumerate() {
            txn.execute(
                "INSERT INTO entries VALUES(?1,?2,?3,?4,?5)",
                params![
                    e.id,
                    e.title,
                    serde_json::to_string(&e.destination)?,
                    i64::try_from(e.revision)?,
                    position as i64
                ],
            )?;
        }
        txn.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    }
    txn.commit()?;
    Ok(saved)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn initialized(f: &crate::test_support::Fixture) -> Connection {
        let db = Connection::open(f.0.join("quick.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL);INSERT INTO meta VALUES('revision',0);CREATE TABLE entries(id TEXT PRIMARY KEY,title TEXT NOT NULL,payload TEXT NOT NULL,revision INTEGER NOT NULL,position INTEGER NOT NULL);").unwrap();
        db
    }
    #[test]
    fn saved_view_conflicts_and_stale_reorder_roll_back_without_touching_files() {
        let f = crate::test_support::Fixture::new("quick");
        let mut db = initialized(&f);
        let mut view = FileView::new(&f.0);
        view.query = "文档".into();
        let a = apply(
            &mut db,
            Action::Save {
                token: 1,
                id: None,
                revision: 0,
                title: "报告".into(),
                view: view.clone(),
            },
        )
        .unwrap()
        .unwrap();
        view.query = "更新".into();
        let b = apply(
            &mut db,
            Action::Save {
                token: 2,
                id: Some(a.id.clone()),
                revision: a.revision,
                title: "更新报告".into(),
                view: view.clone(),
            },
        )
        .unwrap()
        .unwrap();
        assert!(
            apply(
                &mut db,
                Action::Save {
                    token: 3,
                    id: Some(a.id.clone()),
                    revision: a.revision,
                    title: "旧草稿".into(),
                    view
                }
            )
            .is_err()
        );
        assert!(
            apply(
                &mut db,
                Action::Reorder {
                    ids: vec![a.id.clone()],
                    revision: 0
                }
            )
            .is_err()
        );
        assert_eq!(load(&db).unwrap(), vec![b.clone()]);
        let file = f.0.join("preserve.txt");
        std::fs::write(&file, b"keep").unwrap();
        apply(
            &mut db,
            Action::Remove {
                id: b.id,
                revision: b.revision,
            },
        )
        .unwrap();
        assert!(load(&db).unwrap().is_empty());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn storage_recovers_after_unavailable_directory_without_losing_edit_retry() {
        let f = crate::test_support::Fixture::new("quick-retry");
        let directory = f.0.join("settings");
        std::fs::write(&directory, b"temporary blocker").unwrap();
        let (tx, rx) = async_channel::unbounded();
        let store = Store::new(directory.clone(), tx);
        let view = FileView::new(&f.0);
        let save = |token| {
            store
                .send(Action::Save {
                    token,
                    id: None,
                    revision: 0,
                    title: "项目".into(),
                    view: view.clone(),
                })
                .unwrap()
        };
        let wait = |token, success| {
            let deadline = std::time::Instant::now() + Duration::from_secs(6);
            loop {
                match rx.try_recv() {
                    Ok(Event::QuickAccess(Notice::Saved { token: t, result })) if t == token => {
                        assert_eq!(result.is_ok(), success);
                        break;
                    }
                    _ => {
                        assert!(std::time::Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        };
        save(1);
        wait(1, false);
        std::fs::remove_file(&directory).unwrap();
        save(2);
        wait(2, true);
        assert_eq!(load(&open(&directory).unwrap()).unwrap().len(), 1);
    }
}
