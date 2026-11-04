use crate::{
    model::{Event, FrozenSelection},
    operations::move_guard::{self, Target},
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
pub struct Request {
    pub token: u64,
    pub generation: u64,
    pub selection: FrozenSelection,
    pub path: PathBuf,
}
struct Shared {
    pending: Mutex<Option<Request>>,
    changed: Condvar,
    latest: AtomicU64,
    stop: AtomicBool,
}
pub struct Worker(Arc<Shared>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_one();
    }
}
impl Worker {
    pub fn new(tx: async_channel::Sender<Event>) -> Self {
        let state = Arc::new(Shared {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            latest: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });
        let s = state.clone();
        std::thread::Builder::new()
            .name("backleaf-drag-target".into())
            .spawn(move || {
                loop {
                    let r = {
                        let mut pending = s.pending.lock().unwrap();
                        while pending.is_none() && !s.stop.load(Ordering::Acquire) {
                            pending = s.changed.wait(pending).unwrap();
                        }
                        if s.stop.load(Ordering::Acquire) {
                            return;
                        }
                        pending.take().unwrap()
                    };
                    let result = (|| -> anyhow::Result<Target> {
                        let target = Target::capture(r.path)?;
                        let chain = move_guard::ancestors(&target.path)?;
                        let db = rusqlite::Connection::open_with_flags(
                            &r.selection.snapshot.database,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                        )?;
                        // Large selections are fully checked by the task preflight before any move.
                        // Hover feedback must not require holding the pointer while millions of paths resolve.
                        if r.selection.mask.count_before(r.selection.count) <= 32 {
                            let mut q = db.prepare("SELECT raw FROM files WHERE id=?1")?;
                            for id in r.selection.mask.ids_from(0, r.selection.count) {
                                anyhow::ensure!(
                                    !s.stop.load(Ordering::Acquire)
                                        && s.latest.load(Ordering::Acquire) == r.token,
                                    "已取消"
                                );
                                let raw: Vec<u8> = q.query_row([id as i64], |r| r.get(0))?;
                                let name = crate::model::decode_name(&raw);
                                let parent = r
                                    .selection
                                    .snapshot
                                    .parent
                                    .get()
                                    .ok_or_else(|| anyhow::anyhow!("源目录不可用"))?;
                                move_guard::check(&parent.join(name), &chain)?;
                            }
                        }
                        target.verify()?;
                        Ok(target)
                    })()
                    .map_err(|e| e.to_string());
                    if s.latest.load(Ordering::Acquire) == r.token
                        && !s.stop.load(Ordering::Acquire)
                    {
                        let _ = tx.send_blocking(Event::DragTarget {
                            generation: r.generation,
                            token: r.token,
                            result,
                        });
                    }
                }
            })
            .expect("drag validator");
        Self(state)
    }
    pub fn request(&self, r: Request) {
        self.0.latest.store(r.token, Ordering::Release);
        *self.0.pending.lock().unwrap() = Some(r);
        self.0.changed.notify_one();
    }
    pub fn cancel(&self, token: u64) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.pending.lock().unwrap() = None;
    }
}
