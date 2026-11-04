//! Versioned local window intent. A monotonic revision prevents a slow save reopening a closed window.
use crate::{
    file_view::{FileView, WinPath},
    model::Event,
    quick_access::Entry,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchState {
    pub definition: FileView,
    pub saved: Option<Entry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub path: WinPath,
    pub search: Option<SearchState>,
    pub bounds: [f32; 4],
    pub maximized: bool,
}
#[derive(Clone, Debug)]
pub struct Restored {
    pub id: String,
    pub revision: u64,
    pub state: State,
}
pub(crate) fn process_created(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let ok = GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u).is_ok();
        let _ = CloseHandle(process);
        ok.then_some((u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime))
    }
}
pub(crate) fn active(pid: u32, created: u64) -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut code = 0;
                let running = GetExitCodeProcess(handle, &mut code).is_ok() && code == 259;
                let _ = CloseHandle(handle);
                running && process_created(pid) == Some(created)
            }
            Err(error) => {
                error.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0)
            }
        }
    }
}
pub fn instance() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    })
    .clone()
}
fn open(directory: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(directory)?;
    let db = Connection::open(directory.join("windows.sqlite"))?;
    db.busy_timeout(Duration::from_secs(2))?;
    db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;CREATE TABLE IF NOT EXISTS windows(id TEXT PRIMARY KEY,instance TEXT NOT NULL,pid INTEGER NOT NULL,created INTEGER NOT NULL,revision INTEGER NOT NULL,closed INTEGER NOT NULL,state TEXT NOT NULL,updated INTEGER NOT NULL);")?;
    Ok(db)
}
pub fn load(directory: &Path) -> anyhow::Result<Vec<Restored>> {
    let mut connection = open(directory)?;
    // Claim the restore group before another process can read the same dead owner.
    let db = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut q=db.prepare("SELECT id,instance,pid,created,revision,closed,state FROM windows ORDER BY updated DESC LIMIT 256")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                crate::operations::ledger::read_u64(r, 3)?,
                crate::operations::ledger::read_u64(r, 4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(q);
    if rows
        .iter()
        .any(|(_, _, pid, created, _, closed, _)| !*closed && active(*pid, *created))
    {
        return Ok(vec![]);
    }
    let Some((_, latest, ..)) = rows.first() else {
        return Ok(vec![]);
    };
    let mut selected = rows
        .iter()
        .filter(|(_, group, _, _, _, closed, _)| group == latest && !*closed)
        .take(10)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        selected.push(&rows[0]);
    }
    let mut restored = Vec::new();
    for (id, _, _, _, revision, _, json) in selected {
        let state: State = serde_json::from_str(json)?;
        anyhow::ensure!(state.version == 1, "窗口状态版本不兼容");
        anyhow::ensure!(state.bounds.iter().all(|v| v.is_finite()), "窗口位置无效");
        restored.push(Restored {
            id: id.clone(),
            revision: revision
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("窗口版本超出范围"))?,
            state,
        });
    }
    let pid = std::process::id();
    let created = process_created(pid).ok_or_else(|| anyhow::anyhow!("无法核实启动进程身份"))?;
    for window in &restored {
        db.execute("UPDATE windows SET instance=?1,pid=?2,created=?3,revision=?4,closed=0,updated=(SELECT coalesce(max(updated),0)+1 FROM windows) WHERE id=?5",
            params![instance(),pid,i64::try_from(created)?,i64::try_from(window.revision)?,window.id])?;
    }
    db.commit()?;
    Ok(restored)
}
struct Save {
    revision: u64,
    state: State,
    closed: bool,
    ack: Option<async_channel::Sender<()>>,
}
struct Shared {
    pending: Mutex<Option<Save>>,
    changed: Condvar,
    stop: std::sync::atomic::AtomicBool,
}
pub struct Store {
    shared: Arc<Shared>,
}
impl Drop for Store {
    fn drop(&mut self) {
        self.shared
            .stop
            .store(true, std::sync::atomic::Ordering::Release);
        self.shared.changed.notify_one();
    }
}
impl Store {
    pub fn new(directory: PathBuf, id: String, tx: async_channel::Sender<Event>) -> Self {
        let shared = Arc::new(Shared {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            stop: std::sync::atomic::AtomicBool::new(false),
        });
        let s = shared.clone();
        let window = id.clone();
        std::thread::Builder::new().name("backleaf-window-state".into()).spawn(move||{
            let result=(||->anyhow::Result<()>{let mut db=None;let group=instance();let pid=std::process::id();let created=process_created(pid).unwrap_or(0);
                loop{
                    let save={let mut p=s.pending.lock().unwrap();while p.is_none()&&!s.stop.load(std::sync::atomic::Ordering::Acquire){p=s.changed.wait(p).unwrap();}if p.is_none(){return Ok(());}
                        if !p.as_ref().unwrap().closed{let(guard,_)=s.changed.wait_timeout(p,Duration::from_millis(150)).unwrap();p=guard;}p.take().unwrap()};
                    let result=(||->anyhow::Result<()>{if db.is_none(){db=Some(open(&directory)?);}let connection=db.as_ref().unwrap();connection.execute("INSERT INTO windows VALUES(?1,?2,?3,?4,?5,?6,?7,(SELECT coalesce(max(updated),0)+1 FROM windows)) ON CONFLICT(id)DO UPDATE SET instance=excluded.instance,pid=excluded.pid,created=excluded.created,revision=excluded.revision,closed=excluded.closed,state=excluded.state,updated=excluded.updated WHERE windows.revision<excluded.revision",params![window,group,pid,i64::try_from(created)?,i64::try_from(save.revision)?,save.closed,serde_json::to_string(&save.state)?])?;Ok(())})().map_err(|e|e.to_string());
                    if result.is_err(){db=None;}
                    if let Some(ack)=save.ack{let _=ack.try_send(());}
                    let _=tx.send_blocking(Event::WindowStateSaved{revision:save.revision,closed:save.closed,result});
                }
            })();if let Err(error)=result{let _=tx.send_blocking(Event::WindowStateSaved{revision:0,closed:false,result:Err(error.to_string())});}
        }).expect("window state worker");
        Self { shared }
    }
    pub fn save(
        &self,
        revision: u64,
        state: State,
        closed: bool,
        ack: Option<async_channel::Sender<()>>,
    ) {
        let mut p = self.shared.pending.lock().unwrap();
        if p.as_ref().is_none_or(|old| old.revision < revision) {
            *p = Some(Save {
                revision,
                state,
                closed,
                ack,
            });
        }
        self.shared.changed.notify_one();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monotonic_save_cannot_resurrect_a_closed_window_and_keeps_search_intent() {
        let f = crate::test_support::Fixture::new("window-state");
        let (tx, rx) = async_channel::unbounded();
        let store = Store::new(f.0.clone(), "window-a".into(), tx);
        let mut definition = FileView::new(&f.0);
        definition.query = "项目".into();
        definition.extensions = vec!["rs".into()];
        let state = State {
            version: 1,
            path: WinPath::new(&f.0),
            search: Some(SearchState {
                definition,
                saved: None,
            }),
            bounds: [10., 20., 900., 600.],
            maximized: false,
        };
        store.save(2, state.clone(), true, None);
        let wait = |revision| {
            let start = std::time::Instant::now();
            loop {
                assert!(start.elapsed() < Duration::from_secs(5));
                match rx.try_recv() {
                    Ok(Event::WindowStateSaved {
                        revision: r,
                        result,
                        ..
                    }) if r == revision => {
                        result.unwrap();
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        };
        wait(2);
        let mut stale = state.clone();
        stale.search = None;
        store.save(1, stale, false, None);
        wait(1);
        let rows = load(&f.0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].revision, 3);
        assert_eq!(rows[0].state, state);
        assert!(
            load(&f.0).unwrap().is_empty(),
            "a concurrent instance must not claim the same restored window"
        );
    }
}
