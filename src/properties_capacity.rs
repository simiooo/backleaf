//! Read-only, bounded properties scans. Only the latest snapshot reaches the UI.
use crate::operations::windows::identity::{self, Identity};
use std::{
    fs::{self, Metadata},
    os::windows::fs::MetadataExt,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::{Duration, Instant, SystemTime},
};

const INTERVAL: Duration = Duration::from_millis(200);
const MAX_DEPTH: usize = 256;
// Reparse points and cloud/offline placeholders must not trigger traversal/hydration.
const SKIP_ATTRIBUTES: u32 = 0x400 | 0x1000 | 0x40000 | 0x400000;

#[derive(Clone, Default)]
pub struct Snapshot {
    pub version: u64,
    pub bytes: u128,
    pub files: u64,
    pub folders: u64,
    pub unavailable: u64,
    pub skipped: u64,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub initialized: bool,
    pub running: bool,
    pub done: bool,
    pub error: Option<String>,
}
struct Shared {
    latest: Mutex<Snapshot>,
    canceled: AtomicBool,
}
struct Job {
    path: PathBuf,
    expected: Option<Identity>,
    shared: Arc<Shared>,
}
pub struct Scan {
    shared: Arc<Shared>,
    pending: Option<Job>,
    sender: Option<SyncSender<Job>>,
}
impl Drop for Scan {
    fn drop(&mut self) {
        self.shared.canceled.store(true, Ordering::Release);
    }
}
impl Scan {
    #[cfg(test)]
    pub(crate) fn cancellation_probe(&self) -> Box<dyn Fn() -> bool> {
        let shared = self.shared.clone();
        Box::new(move || shared.canceled.load(Ordering::Acquire))
    }
    pub fn new(path: PathBuf, expected: Option<Identity>) -> Self {
        let shared = Arc::new(Shared {
            latest: Mutex::new(Snapshot::default()),
            canceled: AtomicBool::new(false),
        });
        let mut this = Self {
            pending: Some(Job {
                path,
                expected,
                shared: shared.clone(),
            }),
            shared,
            sender: None,
        };
        match pool() {
            Ok(sender) => this.sender = Some(sender),
            Err(error) => {
                publish(
                    &this.shared,
                    Snapshot {
                        done: true,
                        error: Some(error),
                        ..Default::default()
                    },
                );
                this.pending = None;
            }
        }
        this.snapshot();
        this
    }
    pub fn snapshot(&mut self) -> Snapshot {
        if let Some(job) = self.pending.take()
            && let Some(sender) = &self.sender
        {
            match sender.try_send(job) {
                Ok(()) => (),
                Err(TrySendError::Full(job)) => self.pending = Some(job),
                Err(TrySendError::Disconnected(_)) => publish(
                    &self.shared,
                    Snapshot {
                        done: true,
                        error: Some("容量统计服务不可用".into()),
                        ..Default::default()
                    },
                ),
            }
        }
        self.shared.latest.lock().unwrap().clone()
    }
}
fn pool() -> Result<SyncSender<Job>, String> {
    static POOL: OnceLock<Result<SyncSender<Job>, String>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Job>(16);
        let rx = Arc::new(Mutex::new(rx));
        let mut started = 0;
        for slot in 0..2 {
            let rx = rx.clone();
            if std::thread::Builder::new()
                .name(format!("backleaf-properties-{slot}"))
                .spawn(move || {
                    loop {
                        let job = { rx.lock().unwrap().recv() };
                        let Ok(job) = job else { break };
                        scan(job, MAX_DEPTH);
                    }
                })
                .is_ok()
            {
                started += 1;
            }
        }
        if started == 0 {
            Err("无法启动容量统计服务".into())
        } else {
            Ok(tx)
        }
    })
    .clone()
}
fn publish(shared: &Shared, mut value: Snapshot) {
    let mut latest = shared.latest.lock().unwrap();
    value.version = latest.version.saturating_add(1);
    *latest = value;
}
fn blocked(metadata: &Metadata) -> bool {
    metadata.file_attributes() & SKIP_ATTRIBUTES != 0
}
fn scan(job: Job, max_depth: usize) {
    let canceled = || job.shared.canceled.load(Ordering::Acquire);
    if canceled() {
        return;
    }
    let mut value = Snapshot {
        running: true,
        ..Default::default()
    };
    publish(&job.shared, value.clone());
    let result = (|| -> anyhow::Result<()> {
        let metadata = fs::symlink_metadata(&job.path)?;
        value.created = metadata.created().ok();
        value.modified = metadata.modified().ok();
        value.initialized = true;
        if blocked(&metadata) {
            value.skipped = 1;
            return Ok(());
        }
        let before = identity::capture(&job.path)?;
        anyhow::ensure!(
            job.expected
                .as_ref()
                .is_none_or(|id| *id == before.identity),
            "项目已改变，请重新打开详情"
        );
        if !metadata.is_dir() {
            value.files = 1;
            value.bytes = metadata.len() as u128;
        } else {
            let mut stack = vec![fs::read_dir(&job.path)?];
            let mut published = Instant::now();
            while let Some(directory) = stack.last_mut() {
                if canceled() {
                    return Ok(());
                }
                match directory.next() {
                    None => {
                        stack.pop();
                    }
                    Some(Err(_)) => value.unavailable = value.unavailable.saturating_add(1),
                    Some(Ok(entry)) => match entry.metadata() {
                        Err(_) => value.unavailable = value.unavailable.saturating_add(1),
                        Ok(metadata) if blocked(&metadata) => {
                            value.skipped = value.skipped.saturating_add(1)
                        }
                        Ok(metadata) if metadata.is_dir() => {
                            value.folders = value.folders.saturating_add(1);
                            if stack.len() >= max_depth {
                                value.skipped = value.skipped.saturating_add(1);
                            } else {
                                let path = entry.path();
                                // Recheck directory attributes immediately before opening it.
                                match fs::symlink_metadata(&path) {
                                    Ok(m) if blocked(&m) => {
                                        value.skipped = value.skipped.saturating_add(1)
                                    }
                                    Ok(m) if m.is_dir() => match fs::read_dir(path) {
                                        Ok(next) => stack.push(next),
                                        Err(_) => {
                                            value.unavailable = value.unavailable.saturating_add(1)
                                        }
                                    },
                                    _ => value.unavailable = value.unavailable.saturating_add(1),
                                }
                            }
                        }
                        Ok(metadata) => {
                            value.files = value.files.saturating_add(1);
                            value.bytes = value.bytes.saturating_add(metadata.len() as u128);
                        }
                    },
                }
                if published.elapsed() >= INTERVAL {
                    publish(&job.shared, value.clone());
                    published = Instant::now();
                }
            }
        }
        if canceled() {
            return Ok(());
        }
        let after = identity::capture(&job.path)?;
        anyhow::ensure!(
            after.identity == before.identity,
            "项目已改变，请重新打开详情"
        );
        Ok(())
    })();
    if canceled() {
        return;
    }
    if let Err(error) = result {
        value.error = Some(error.to_string());
    }
    value.running = false;
    value.done = true;
    publish(&job.shared, value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Fixture;
    fn make_job(path: PathBuf) -> Job {
        Job {
            path,
            expected: None,
            shared: Arc::new(Shared {
                latest: Mutex::new(Snapshot::default()),
                canceled: AtomicBool::new(false),
            }),
        }
    }
    fn result(job: Job, depth: usize) -> Snapshot {
        let shared = job.shared.clone();
        scan(job, depth);
        shared.latest.lock().unwrap().clone()
    }
    #[test]
    fn nested_unicode_and_empty_folders_use_logical_metadata_sizes() {
        let f = Fixture::new("properties-size");
        fs::create_dir_all(f.0.join("中文/empty")).unwrap();
        fs::write(f.0.join("a.txt"), b"abc").unwrap();
        let large = fs::File::create(f.0.join("中文/large.bin")).unwrap();
        large.set_len(8 * 1024 * 1024 + 7).unwrap();
        drop(large);
        let v = result(make_job(f.0.clone()), MAX_DEPTH);
        assert!(v.done && v.initialized && v.error.is_none());
        assert_eq!((v.bytes, v.files, v.folders), (8 * 1024 * 1024 + 10, 2, 2));
        assert_eq!((v.skipped, v.unavailable), (0, 0));
        assert!(v.modified.is_some());
        let empty = result(make_job(f.0.join("中文/empty")), MAX_DEPTH);
        assert!(empty.done && empty.error.is_none());
        assert_eq!(empty.bytes, 0);
    }
    #[test]
    fn junctions_and_cycles_do_not_expand_outside_the_requested_tree() {
        let f = Fixture::new("properties-junction");
        let root = f.0.join("tree");
        let outside = f.0.join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join("inside"), b"abc").unwrap();
        fs::write(outside.join("excluded"), b"outside payload").unwrap();
        for (name, target) in [("external", &outside), ("loop", &root)] {
            let output = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(root.join(name))
                .arg(target)
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output);
        }
        let v = result(make_job(root.clone()), MAX_DEPTH);
        assert_eq!((v.bytes, v.files, v.skipped), (3, 1, 2));
        assert!(v.done);
        fs::remove_dir(root.join("external")).unwrap();
        fs::remove_dir(root.join("loop")).unwrap();
    }
    #[test]
    fn replacement_identity_and_missing_paths_report_failure() {
        let f = Fixture::new("properties-identity");
        let path = f.0.join("item");
        fs::write(&path, b"before").unwrap();
        let mut job = make_job(path.clone());
        job.expected = Some(identity::capture(&path).unwrap().identity);
        fs::rename(&path, f.0.join("old")).unwrap();
        fs::write(&path, b"after").unwrap();
        let v = result(job, MAX_DEPTH);
        assert!(v.error.unwrap().contains("改变"));
        assert_eq!(v.bytes, 0);
        let missing = result(make_job(f.0.join("missing")), MAX_DEPTH);
        assert!(missing.done && !missing.initialized && missing.error.is_some());
    }
    #[test]
    fn cancellation_does_not_start_or_publish_a_finished_scan() {
        let job = make_job(PathBuf::from("not-an-accessed-path"));
        job.shared.canceled.store(true, Ordering::Release);
        let v = result(job, MAX_DEPTH);
        assert!(!v.done);
        assert_eq!(v.version, 0);
    }
    #[test]
    fn depth_limit_marks_an_incomplete_tree_instead_of_recursing_forever() {
        let f = Fixture::new("properties-depth");
        fs::create_dir_all(f.0.join("a/b/c")).unwrap();
        fs::write(f.0.join("root"), b"x").unwrap();
        fs::write(f.0.join("a/b/c/excluded"), b"xx").unwrap();
        let v = result(make_job(f.0.clone()), 2);
        assert_eq!((v.files, v.bytes, v.skipped), (1, 1, 1));
        assert!(v.done);
    }
    #[test]
    fn many_entries_coalesce_publications_and_keep_the_exact_final_sum() {
        let f = Fixture::new("properties-publication");
        for n in 0..1000 {
            fs::write(f.0.join(n.to_string()), b"x").unwrap();
        }
        let started = Instant::now();
        let v = result(make_job(f.0.clone()), MAX_DEPTH);
        assert_eq!((v.bytes, v.files), (1000, 1000));
        assert!(u128::from(v.version) <= started.elapsed().as_millis() / 200 + 2);
    }
    #[test]
    fn full_queue_keeps_one_pending_job_without_blocking_or_losing_it() {
        let f = Fixture::new("properties-queue");
        fs::write(f.0.join("file"), b"abc").unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(tx.send(make_job(f.0.clone())).is_ok());
        let job = make_job(f.0.clone());
        let mut worker = Scan {
            shared: job.shared.clone(),
            pending: Some(job),
            sender: Some(tx),
        };
        assert!(!worker.snapshot().running);
        assert!(worker.pending.is_some());
        drop(rx.recv().unwrap());
        worker.snapshot();
        assert!(worker.pending.is_none());
        scan(rx.recv().unwrap(), MAX_DEPTH);
        let v = worker.snapshot();
        assert!(v.done);
        assert_eq!((v.bytes, v.files), (3, 1));
    }
}
