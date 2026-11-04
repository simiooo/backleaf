//! Bounded task registry, fair batch reservations, coalesced UI wakeups and a latest-only result reader.
use super::{
    clipboard::Clipboard,
    ledger::{Counts, Ledger, Row},
    prepare::Preparation,
    windows::{self as shell, Apartment, Collision, Kind, Notice, Observer, Step},
};
use crate::{model::FrozenSelection, selection::SelectionMask};
use roaring::RoaringTreemap;
use std::{
    collections::VecDeque,
    ffi::OsString,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Queued,
    Preparing,
    Running,
    Finishing,
    Canceling,
    Complete,
    Partial,
    Canceled,
    Failed,
    Unknown,
}
impl Phase {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Partial | Self::Canceled | Self::Failed | Self::Unknown
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Copy,
    Move,
    Recycle,
    Rename,
    NewFolder,
    ClipboardCopy,
    ClipboardCut,
    CopyPaths,
}
impl Operation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "复制",
            Self::Move => "移动",
            Self::Recycle => "移至回收站",
            Self::Rename => "重命名",
            Self::NewFolder => "新建文件夹",
            Self::ClipboardCopy => "复制到剪贴板",
            Self::ClipboardCut => "剪切",
            Self::CopyPaths => "复制路径",
        }
    }
}
#[derive(Clone)]
pub struct ActiveName {
    pub key: u64,
    pub token: u64,
    pub name: Arc<str>,
}
#[derive(Clone)]
pub struct Page {
    pub after: u64,
    pub failed_only: bool,
    pub rows: Arc<Vec<Row>>,
    pub error: Option<Arc<str>>,
}
#[derive(Clone)]
pub struct View {
    pub id: u64,
    pub operation: Operation,
    pub phase: Phase,
    pub started: Instant,
    pub tree: bool,
    pub target: Arc<PathBuf>,
    pub ledger: Arc<PathBuf>,
    pub total: Option<u64>,
    pub prepared: u64,
    pub successful: u64,
    pub failed: u64,
    pub unknown: u64,
    pub bytes: u64,
    pub in_flight: u64,
    pub size_available: bool,
    pub bytes_total: Option<u64>,
    pub active: Vec<ActiveName>,
    pub error: Option<Arc<str>>,
    pub created: Option<Arc<PathBuf>>,
    pub page: Option<Page>,
    pub source_snapshot: Option<Arc<PathBuf>>,
    pub source_mask: Option<SelectionMask>,
    pub removed: Arc<RoaringTreemap>,
}
struct Wake {
    tx: async_channel::Sender<()>,
    started: Instant,
    last: AtomicU64,
    visible: AtomicBool,
}
impl Wake {
    fn send(&self, force: bool) {
        let now = self.started.elapsed().as_millis() as u64;
        let interval = if self.visible.load(Ordering::Relaxed) {
            100
        } else {
            1000
        };
        let old = self.last.load(Ordering::Relaxed);
        if force
            || (now.saturating_sub(old) >= interval
                && self
                    .last
                    .compare_exchange(old, now, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok())
        {
            if force {
                self.last.store(now, Ordering::Release);
            }
            let _ = self.tx.try_send(());
        }
    }
}
struct Task {
    view: Mutex<View>,
    cancel: AtomicBool,
    persist_failed: AtomicBool,
    wake: Arc<Wake>,
    page_token: AtomicU64,
}
impl Task {
    fn update(&self, force: bool, f: impl FnOnce(&mut View)) {
        f(&mut self.view.lock().unwrap());
        self.wake.send(force);
    }
    fn canceled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
    fn fail(&self, error: impl std::fmt::Display) {
        let failed = self.persist_failed.load(Ordering::Acquire);
        self.update(true, |v| {
            v.phase = if failed {
                Phase::Unknown
            } else if self.canceled() {
                Phase::Canceled
            } else {
                Phase::Failed
            };
            v.active.clear();
            v.in_flight = 0;
            v.error = Some(
                error
                    .to_string()
                    .chars()
                    .take(1024)
                    .collect::<String>()
                    .into(),
            );
        });
    }
    fn counts(&self, counts: &Counts) {
        self.update(false, |v| {
            v.successful = counts.successful;
            v.failed = counts.failed;
            v.unknown = counts.unknown
                + counts.partial
                + counts.not_processed
                + counts.skipped
                + counts.canceled;
        });
    }
}
#[derive(Default)]
struct Gate {
    state: Mutex<(Option<u64>, VecDeque<u64>)>,
    changed: Condvar,
}
struct Permit<'a> {
    gate: &'a Gate,
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.gate.state.lock().unwrap().0 = None;
        self.gate.changed.notify_all();
    }
}
impl Gate {
    fn acquire(&self, id: u64, task: &Task) -> anyhow::Result<Permit<'_>> {
        let mut state = self.state.lock().unwrap();
        state.1.push_back(id);
        loop {
            if task.canceled() {
                state.1.retain(|key| *key != id);
                self.changed.notify_all();
                anyhow::bail!("已取消");
            }
            if state.0.is_none() && state.1.front() == Some(&id) {
                state.1.pop_front();
                state.0 = Some(id);
                return Ok(Permit { gate: self });
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }
}
#[derive(Clone)]
pub struct OwnClipboard {
    pub sequence: u32,
    pub ledger: Arc<PathBuf>,
    pub cut: bool,
    pub task: u64,
}
struct ReadRequest {
    task: Weak<Task>,
    token: u64,
    after: u64,
    failed_only: bool,
}
#[derive(Default)]
struct Reader {
    pending: Mutex<Option<ReadRequest>>,
    changed: Condvar,
    stop: AtomicBool,
}
struct Shared {
    tasks: Mutex<Vec<Arc<Task>>>,
    next: AtomicU64,
    directory: PathBuf,
    wake: Arc<Wake>,
    files: Gate,
    clipboard: Gate,
    own: Mutex<Option<OwnClipboard>>,
    clipboard_intent: AtomicU64,
    reader: Arc<Reader>,
}
impl Drop for Shared {
    fn drop(&mut self) {
        self.reader.stop.store(true, Ordering::Release);
        self.reader.changed.notify_all();
    }
}
#[derive(Clone)]
pub struct Hub {
    shared: Arc<Shared>,
}
impl Hub {
    pub fn new(directory: PathBuf, tx: async_channel::Sender<()>) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let reader = Arc::new(Reader::default());
        let shared = Arc::new(Shared {
            tasks: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            directory: directory.join(format!("session-{}-{stamp}", std::process::id())),
            wake: Arc::new(Wake {
                tx,
                started: Instant::now(),
                last: AtomicU64::new(0),
                visible: AtomicBool::new(true),
            }),
            files: Gate::default(),
            clipboard: Gate::default(),
            own: Mutex::new(None),
            clipboard_intent: AtomicU64::new(0),
            reader: reader.clone(),
        });
        std::thread::Builder::new()
            .name("backleaf-result-reader".into())
            .spawn(move || read_loop(reader))
            .expect("result reader");
        Self { shared }
    }
    pub fn views(&self) -> Vec<View> {
        self.shared
            .tasks
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.view.lock().unwrap().clone())
            .collect()
    }
    pub fn view(&self, id: u64) -> Option<View> {
        self.task(id).map(|t| t.view.lock().unwrap().clone())
    }
    fn task(&self, id: u64) -> Option<Arc<Task>> {
        self.shared
            .tasks
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.view.lock().unwrap().id == id)
            .cloned()
    }
    pub fn set_visible(&self, visible: bool) {
        self.shared.wake.visible.store(visible, Ordering::Release);
    }
    pub fn active(&self) -> bool {
        self.shared
            .tasks
            .lock()
            .unwrap()
            .iter()
            .any(|t| !t.view.lock().unwrap().phase.terminal())
    }
    pub fn cancel(&self, id: u64) {
        if let Some(task) = self.task(id) {
            let terminal = task.view.lock().unwrap().phase.terminal();
            if terminal {
                return;
            }
            task.cancel.store(true, Ordering::Release);
            task.update(true, |v| v.phase = Phase::Canceling);
            self.shared.files.changed.notify_all();
            self.shared.clipboard.changed.notify_all();
        }
    }
    pub fn cancel_all(&self) {
        for view in self.views() {
            self.cancel(view.id);
        }
    }
    pub fn dismiss(&self, id: u64) {
        self.shared.tasks.lock().unwrap().retain(|t| {
            let v = t.view.lock().unwrap();
            v.id != id || !v.phase.terminal()
        });
        self.shared.wake.send(true);
    }
    pub fn own_clipboard(&self) -> Option<OwnClipboard> {
        self.observe_clipboard();
        self.shared.own.lock().unwrap().clone()
    }
    pub fn observe_clipboard(&self) {
        let now = Clipboard::sequence();
        let mut own = self.shared.own.lock().unwrap();
        if own.as_ref().is_some_and(|c| c.sequence != now) {
            *own = None;
        }
    }
    pub fn clipboard_preparing(&self) -> bool {
        let id = self.shared.clipboard_intent.load(Ordering::Acquire);
        self.view(id).is_some_and(|v| !v.phase.terminal())
    }
    fn register(
        &self,
        operation: Operation,
        target: PathBuf,
        selection: Option<&FrozenSelection>,
    ) -> anyhow::Result<Arc<Task>> {
        let mut tasks = self.shared.tasks.lock().unwrap();
        anyhow::ensure!(tasks.len() < 24, "任务已满，请先关闭已完成任务");
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let task = Arc::new(Task {
            view: Mutex::new(View {
                id,
                operation,
                phase: Phase::Queued,
                started: Instant::now(),
                tree: false,
                target: Arc::new(target),
                ledger: Arc::new(self.shared.directory.join(format!("task-{id}.sqlite"))),
                total: selection.map(|s| s.mask.count_before(s.count) as u64),
                prepared: 0,
                successful: 0,
                failed: 0,
                unknown: 0,
                bytes: 0,
                in_flight: 0,
                size_available: false,
                bytes_total: None,
                active: vec![],
                error: None,
                created: None,
                page: None,
                source_snapshot: selection.map(|s| Arc::new(s.snapshot.database.clone())),
                source_mask: selection.map(|s| s.mask.clone()),
                removed: Arc::new(RoaringTreemap::new()),
            }),
            cancel: AtomicBool::new(false),
            persist_failed: AtomicBool::new(false),
            wake: self.shared.wake.clone(),
            page_token: AtomicU64::new(0),
        });
        tasks.push(task.clone());
        drop(tasks);
        self.shared.wake.send(true);
        Ok(task)
    }
    fn launch(
        &self,
        task: Arc<Task>,
        body: impl FnOnce(Arc<Shared>, Arc<Task>) -> anyhow::Result<()> + Send + 'static,
    ) -> u64 {
        let id = task.view.lock().unwrap().id;
        let shared = self.shared.clone();
        let worker = task.clone();
        if let Err(error) = std::thread::Builder::new()
            .name(format!("backleaf-task-{id}"))
            .spawn(move || {
                if let Err(error) = body(shared, worker.clone()) {
                    let path = worker.view.lock().unwrap().ledger.clone();
                    if path.exists() {
                        match Ledger::open(&path).and_then(|ledger| {
                            let uncertain = ledger.interrupt_unverified()?;
                            worker.counts(&ledger.counts()?);
                            Ok(uncertain)
                        }) {
                            Ok(true) | Err(_) => {
                                worker.persist_failed.store(true, Ordering::Release)
                            }
                            Ok(false) => (),
                        }
                    }
                    worker.fail(error);
                }
            })
        {
            task.fail(error);
        }
        id
    }
    pub fn selection(
        &self,
        selection: FrozenSelection,
        operation: Operation,
        target: PathBuf,
        owner: usize,
    ) -> anyhow::Result<u64> {
        let task = self.register(operation, target.clone(), Some(&selection))?;
        Ok(self.launch(task, move |shared, task| {
            let id = task.view.lock().unwrap().id;
            let ledger = create_ledger(&task)?;
            let mut prepare = Preparation::new(selection, None)?;
            loop {
                let permit = shared.files.acquire(id, &task)?;
                task.update(false, |v| v.phase = Phase::Preparing);
                let progress = prepare.advance(&ledger, || task.canceled())?;
                task.update(false, |v| {
                    v.prepared = progress.prepared;
                    v.total = Some(progress.total);
                    v.bytes_total =
                        (!progress.has_directories).then_some(progress.known_file_bytes);
                    v.tree = progress.has_directories;
                    v.size_available = !v.tree;
                });
                drop(permit);
                if progress.canceled {
                    anyhow::bail!("已取消");
                }
                if progress.complete {
                    break;
                }
            }
            let kind = match operation {
                Operation::Copy => Kind::Copy,
                Operation::Move => Kind::Move,
                Operation::Recycle => Kind::Recycle,
                _ => anyhow::bail!("Invalid selection operation"),
            };
            run_files(&shared, &task, ledger, kind, &target, None, owner)?;
            finish(&task);
            Ok(())
        }))
    }
    pub fn clipboard(
        &self,
        selection: FrozenSelection,
        cut: bool,
        paths: bool,
    ) -> anyhow::Result<u64> {
        let operation = if paths {
            Operation::CopyPaths
        } else if cut {
            Operation::ClipboardCut
        } else {
            Operation::ClipboardCopy
        };
        let task = self.register(operation, PathBuf::from("剪贴板"), Some(&selection))?;
        let id = task.view.lock().unwrap().id;
        let previous = self.shared.clipboard_intent.swap(id, Ordering::AcqRel);
        if previous != 0 {
            self.cancel(previous);
        }
        let expected = Clipboard::sequence();
        Ok(self.launch(task, move |shared, task| {
            let _permit = shared.clipboard.acquire(id, &task)?;
            let clipboard = Clipboard::new()?;
            let ledger = create_ledger(&task)?;
            let mut prepare = Preparation::new(
                selection,
                (!paths).then_some(super::clipboard::MAX_PAYLOAD_BYTES as u64),
            )?;
            loop {
                task.update(false, |v| v.phase = Phase::Preparing);
                let progress = prepare.advance(&ledger, || {
                    task.canceled() || shared.clipboard_intent.load(Ordering::Acquire) != id
                })?;
                task.update(false, |v| {
                    v.prepared = progress.prepared;
                    v.total = Some(progress.total);
                });
                clipboard.pump();
                if progress.canceled {
                    anyhow::bail!("已取消");
                }
                if progress.complete {
                    break;
                }
            }
            let now = Clipboard::sequence();
            let own = shared.own.lock().unwrap().clone();
            let expected = if own
                .as_ref()
                .is_some_and(|c| c.sequence == now && c.task < id)
            {
                now
            } else {
                expected
            };
            let canceled =
                || task.canceled() || shared.clipboard_intent.load(Ordering::Acquire) != id;
            let sequence = if paths {
                clipboard.write_text_paths(&ledger, Some(expected), canceled)?
            } else {
                clipboard.write_files(&ledger, cut, false, Some(expected), canceled)?
            };
            let manifest = task.view.lock().unwrap().ledger.clone();
            *shared.own.lock().unwrap() = (!paths).then_some(OwnClipboard {
                sequence,
                ledger: manifest,
                cut,
                task: id,
            });
            task.update(true, |v| {
                v.phase = Phase::Complete;
                v.successful = v.prepared;
            });
            Ok(())
        }))
    }
    pub fn paste(
        &self,
        target: PathBuf,
        expected_target: Option<shell::identity::Identity>,
        owner: usize,
    ) -> anyhow::Result<u64> {
        anyhow::ensure!(!self.clipboard_preparing(), "正在准备剪贴板");
        let expected = Clipboard::sequence();
        let task = self.register(Operation::Copy, target.clone(), None)?;
        Ok(self.launch(task, move |shared, task| {
            let id = task.view.lock().unwrap().id;
            let clipboard_permit = shared.clipboard.acquire(id, &task)?;
            let clipboard = Clipboard::new()?;
            let ledger = create_ledger(&task)?;
            task.update(true, |v| v.phase = Phase::Preparing);
            let own = shared
                .own
                .lock()
                .unwrap()
                .clone()
                .filter(|c| c.sequence == expected);
            let input = if let Some(own) = own {
                clipboard.read_own_files(
                    &ledger,
                    &Ledger::read_only(&own.ledger)?,
                    expected,
                    own.cut,
                    || task.canceled(),
                    |prepared, total| {
                        task.update(false, |v| {
                            v.prepared = prepared;
                            v.total = Some(total);
                        })
                    },
                )?
            } else {
                clipboard.read_files(
                    &ledger,
                    expected,
                    || task.canceled(),
                    |prepared, total| {
                        task.update(false, |v| {
                            v.prepared = prepared;
                            v.total = Some(total);
                        })
                    },
                )?
            };
            task.update(false, |v| {
                v.total = Some(input.count);
                v.operation = if input.move_requested {
                    Operation::Move
                } else {
                    Operation::Copy
                };
                v.bytes_total = (!input.has_directories).then_some(input.known_file_bytes);
                v.tree = input.has_directories;
                v.size_available = !v.tree;
            });
            drop(clipboard_permit);
            let kind = if input.move_requested {
                Kind::Move
            } else {
                Kind::Copy
            };
            run_files_with_target(
                &shared,
                &task,
                ledger,
                kind,
                &target,
                None,
                expected_target,
                owner,
            )?;
            if input.move_requested && Clipboard::sequence() == input.sequence {
                let permit = shared.clipboard.acquire(id, &task);
                if let Ok(_permit) = permit {
                    let ledger = Ledger::read_only(&task.view.lock().unwrap().ledger)?;
                    let mut left = 0;
                    ledger.visit_roots(true, |_| {
                        left += 1;
                        Ok(())
                    })?;
                    if Clipboard::sequence() == expected {
                        let result = if left == 0 {
                            clipboard.clear_if_sequence(expected).map(|_| None)
                        } else {
                            clipboard
                                .write_files(&ledger, true, true, Some(expected), || {
                                    task.canceled()
                                })
                                .map(Some)
                        };
                        match result {
                            Ok(sequence) => {
                                let path = task.view.lock().unwrap().ledger.clone();
                                *shared.own.lock().unwrap() =
                                    sequence.map(|sequence| OwnClipboard {
                                        sequence,
                                        ledger: path,
                                        cut: true,
                                        task: id,
                                    });
                            }
                            Err(error) => {
                                if Clipboard::sequence() == expected {
                                    task.update(true, |v| {
                                        v.error = Some(
                                            format!("文件结果已保留，剪贴板未同步：{error}").into(),
                                        )
                                    });
                                }
                            }
                        }
                    }
                }
            }
            drop(input);
            finish(&task);
            Ok(())
        }))
    }
    pub fn direct(
        &self,
        source: PathBuf,
        destination: PathBuf,
        new_name: OsString,
        expected: Option<shell::identity::Identity>,
        new_folder: bool,
        owner: usize,
    ) -> anyhow::Result<u64> {
        let operation = if new_folder {
            Operation::NewFolder
        } else {
            Operation::Rename
        };
        let task = self.register(operation, destination.clone(), None)?;
        Ok(self.launch(task, move |shared, task| {
            let id = task.view.lock().unwrap().id;
            let _permit = shared.files.acquire(id, &task)?;
            let ledger = create_ledger(&task)?;
            let metadata =
                shell::identity::capture(if new_folder { &destination } else { &source })?;
            task.update(false, |v| {
                v.total = Some(1);
                v.bytes_total = metadata.bytes;
                v.size_available = metadata.bytes.is_some();
            });
            let root = if new_folder {
                destination.join(&new_name)
            } else {
                source.clone()
            };
            ledger.add_root(0, &root, Some(&metadata))?;
            let destination_identity = if new_folder {
                Some(metadata.identity.clone())
            } else {
                None
            };
            let step = Step {
                key: 0,
                source,
                destination,
                new_name: Some(new_name),
                expected: expected.or(Some(metadata.identity)),
                expected_destination: destination_identity,
            };
            let kind = if new_folder {
                Kind::NewFolder
            } else {
                Kind::Rename
            };
            let apartment = Apartment::new()?;
            execute_batch(
                &task,
                ledger,
                kind,
                &[step],
                if new_folder {
                    Collision::KeepBoth
                } else {
                    Collision::Ask
                },
                owner,
                &apartment,
            )?;
            finish(&task);
            Ok(())
        }))
    }
    pub fn clear_cut(&self) -> anyhow::Result<()> {
        if let Some(own) = self.own_clipboard()
            && own.cut
        {
            let task = self.register(Operation::ClipboardCopy, PathBuf::from("剪贴板"), None)?;
            let id = task.view.lock().unwrap().id;
            self.launch(task, move |shared, task| {
                let _permit = shared.clipboard.acquire(id, &task)?;
                let clipboard = Clipboard::new()?;
                clipboard.clear_if_sequence(own.sequence)?;
                *shared.own.lock().unwrap() = None;
                task.update(true, |v| v.phase = Phase::Complete);
                Ok(())
            });
        }
        Ok(())
    }
    pub fn page(&self, id: u64, after: u64, failed_only: bool) {
        if let Some(task) = self.task(id) {
            let token = task.page_token.fetch_add(1, Ordering::AcqRel) + 1;
            *self.shared.reader.pending.lock().unwrap() = Some(ReadRequest {
                task: Arc::downgrade(&task),
                token,
                after,
                failed_only,
            });
            self.shared.reader.changed.notify_one();
        }
    }
}
fn create_ledger(task: &Task) -> anyhow::Result<Ledger> {
    let path = task.view.lock().unwrap().ledger.clone();
    std::fs::create_dir_all(path.parent().unwrap())?;
    Ledger::open(&path)
}
fn finish(task: &Task) {
    task.update(true, |v| {
        v.active.clear();
        v.in_flight = 0;
        v.phase = if task.persist_failed.load(Ordering::Acquire) {
            Phase::Unknown
        } else if task.canceled() {
            Phase::Canceled
        } else if v.unknown > 0 {
            Phase::Partial
        } else if v.failed > 0 {
            if v.successful > 0 {
                Phase::Partial
            } else {
                Phase::Failed
            }
        } else {
            Phase::Complete
        };
    });
}
fn run_files(
    shared: &Shared,
    task: &Arc<Task>,
    ledger: Ledger,
    kind: Kind,
    target: &Path,
    name: Option<&OsString>,
    owner: usize,
) -> anyhow::Result<()> {
    let expected = if matches!(kind, Kind::Copy | Kind::Move) {
        Some(shell::identity::capture(target)?.identity)
    } else {
        None
    };
    run_files_with_target(shared, task, ledger, kind, target, name, expected, owner)
}
#[expect(
    clippy::too_many_arguments,
    reason = "Shell execution carries an explicit destination identity and owner HWND alongside the durable task"
)]
fn run_files_with_target(
    shared: &Shared,
    task: &Arc<Task>,
    ledger: Ledger,
    kind: Kind,
    target: &Path,
    name: Option<&OsString>,
    expected: Option<shell::identity::Identity>,
    owner: usize,
) -> anyhow::Result<()> {
    let apartment = Apartment::new()?;
    let mut next = 0;
    let id = task.view.lock().unwrap().id;
    let mut ledger = ledger;
    loop {
        if task.canceled() {
            ledger.close_pending()?;
            break;
        }
        let permit = shared.files.acquire(id, task)?;
        if let Some(expected) = &expected {
            anyhow::ensure!(
                shell::identity::capture(target)?.identity == *expected,
                "目标已改变，请重新选择"
            );
        }
        let mut steps = ledger.pending_batch(next, target, name)?;
        if steps.is_empty() {
            drop(permit);
            break;
        }
        let self_copy = |step: &Step| {
            matches!(kind, Kind::Copy)
                && expected.as_ref().is_some_and(|target| {
                    step.source
                        .parent()
                        .and_then(|p| shell::identity::capture(p).ok())
                        .is_some_and(|m| m.identity == *target)
                })
        };
        let duplicate = self_copy(&steps[0]);
        let take = steps
            .iter()
            .position(|s| self_copy(s) != duplicate)
            .unwrap_or(steps.len());
        steps.truncate(take);
        for step in &mut steps {
            step.expected_destination = expected.clone();
        }
        next = steps.last().unwrap().key + 1;
        ledger = execute_batch(
            task,
            ledger,
            kind,
            &steps,
            if duplicate {
                Collision::KeepBoth
            } else {
                Collision::Ask
            },
            owner,
            &apartment,
        )?;
        drop(permit);
    }
    let counts = ledger.counts()?;
    task.counts(&counts);
    if counts.failed
        + counts.unknown
        + counts.partial
        + counts.canceled
        + counts.not_processed
        + counts.skipped
        == 0
    {
        task.update(false, |v| {
            v.total = Some(counts.discovered);
            if v.size_available {
                v.bytes_total = Some(v.bytes);
            }
        });
    }
    task.update(false, |v| v.phase = Phase::Finishing);
    Ok(())
}
struct Reporter {
    task: Arc<Task>,
    ledger: Mutex<Option<Ledger>>,
    last_commit: Mutex<Instant>,
    kind: Kind,
    active: Mutex<VecDeque<(u64, ActiveName)>>,
    measure: Mutex<ByteProgress>,
}
#[derive(Default)]
struct ByteProgress {
    completed: u64,
    reported: u64,
    start: Option<u64>,
    in_flight: u64,
}
impl ByteProgress {
    fn begin(&mut self, file: bool) {
        if file {
            self.start = Some(self.reported);
            self.in_flight = 0;
        }
    }
    fn report(&mut self, value: u64) {
        self.reported = value;
        if let Some(start) = self.start {
            self.in_flight = value.saturating_sub(start);
        }
    }
    fn end(&mut self, confirmed: Option<u64>) -> bool {
        let valid = if let Some(size) = confirmed {
            if let Some(value) = self.completed.checked_add(size) {
                self.completed = value;
                true
            } else {
                false
            }
        } else {
            true
        };
        self.start = None;
        self.in_flight = 0;
        valid
    }
}
impl Observer for Reporter {
    fn canceled(&self) -> bool {
        self.task.canceled()
    }
    fn notice(&self, notice: Notice) {
        let mut locked = self.ledger.lock().unwrap();
        let ledger = locked.as_mut().unwrap();
        if let Err(error) = ledger.record(&notice) {
            self.task.persist_failed.store(true, Ordering::Release);
            self.task.cancel.store(true, Ordering::Release);
            self.task.update(true, |v| {
                v.phase = Phase::Canceling;
                v.error = Some(format!("无法记录结果：{error}").into());
            });
            return;
        }
        if self.last_commit.lock().unwrap().elapsed() >= Duration::from_millis(50) {
            if ledger.commit().and_then(|_| ledger.begin()).is_err() {
                self.task.persist_failed.store(true, Ordering::Release);
                self.task.cancel.store(true, Ordering::Release);
            }
            *self.last_commit.lock().unwrap() = Instant::now();
        }
        let counts = ledger.counts().ok();
        drop(locked);
        let mut active = self.active.lock().unwrap();
        let mut measure = self.measure.lock().unwrap();
        match &notice {
            Notice::Before {
                key,
                source: Some(path),
                ..
            } => {
                let is_file = shell::identity::capture(path).is_ok_and(|m| !m.directory);
                measure.begin(is_file);
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                path.hash(&mut hash);
                let token = hash.finish();
                active.retain(|(k, _)| *k != token);
                active.push_back((
                    token,
                    ActiveName {
                        key: *key,
                        token,
                        name: path
                            .file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                            .into(),
                    },
                ));
                while active.len() > 8 {
                    active.pop_front();
                }
            }
            Notice::After {
                key,
                source: Some(path),
                root,
                created,
                hresult,
            } => {
                let succeeded =
                    matches!(*hresult as u32, 0 | 0x270008 | 0x27000a) && created.is_some();
                let confirmed = if succeeded {
                    created
                        .as_ref()
                        .and_then(|p| shell::identity::capture(p).ok())
                        .and_then(|m| m.bytes)
                } else {
                    None
                };
                let valid = measure.end(confirmed);
                self.task.update(false, |v| {
                    if confirmed.is_some() {
                        v.size_available = true;
                    }
                    if !valid {
                        v.size_available = false;
                        v.bytes_total = None;
                    }
                });
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                path.hash(&mut hash);
                let token = hash.finish();
                active.retain(|(k, _)| *k != token);
                self.task.update(false, |v| {
                    if *root && *hresult >= 0 && created.is_some() {
                        v.created = created.clone().map(Arc::new);
                        if matches!(self.kind, Kind::Move | Kind::Recycle)
                            && matches!(*hresult as u32, 0 | 0x270008 | 0x27000a)
                            && !path.try_exists().unwrap_or(true)
                        {
                            Arc::make_mut(&mut v.removed).insert(*key);
                        }
                    }
                    if *hresult < 0
                        && !matches!(*hresult as u32, 0x80004004 | 0x80270000 | 0x80270001)
                    {
                        v.error = Some(error_label(*hresult).into());
                    }
                });
            }
            Notice::Progress {
                bytes, bytes_total, ..
            } => {
                measure.report(*bytes);
                if *bytes_total > 0 {
                    self.task.update(false, |v| v.size_available = true);
                }
            }
            _ => {}
        }
        let rows = active.iter().map(|(_, row)| row.clone()).collect();
        drop(active);
        self.task.update(false, |v| v.active = rows);
        self.task.update(false, |v| {
            v.bytes = measure.completed;
            v.in_flight = measure.in_flight;
        });
        if let Some(counts) = counts {
            self.task.counts(&counts);
        }
    }
}
fn execute_batch(
    task: &Arc<Task>,
    ledger: Ledger,
    kind: Kind,
    steps: &[Step],
    collision: Collision,
    owner: usize,
    apartment: &Apartment,
) -> anyhow::Result<Ledger> {
    let keys = steps.iter().map(|s| s.key).collect::<Vec<_>>();
    ledger.mark_dispatched(&keys)?;
    ledger.begin()?;
    task.update(false, |v| {
        v.phase = Phase::Running;
        if v.tree {
            v.total = None;
        }
    });
    let completed = task.view.lock().unwrap().bytes;
    let reporter = Arc::new(Reporter {
        task: task.clone(),
        ledger: Mutex::new(Some(ledger)),
        last_commit: Mutex::new(Instant::now()),
        kind,
        active: Mutex::new(VecDeque::new()),
        measure: Mutex::new(ByteProgress {
            completed,
            ..Default::default()
        }),
    });
    let outcome = shell::perform(apartment, kind, steps, collision, owner, reporter.clone())?;
    if let Some((_, code)) = outcome.queue_errors.first() {
        task.update(false, |v| v.error = Some(error_label(*code).into()));
    }
    let ledger = reporter.ledger.lock().unwrap().take().unwrap();
    ledger.finish_batch(&keys, &outcome, task.canceled())?;
    task.counts(&ledger.counts()?);
    Ok(ledger)
}
fn read_loop(reader: Arc<Reader>) {
    loop {
        let request = {
            let mut pending = reader.pending.lock().unwrap();
            while pending.is_none() && !reader.stop.load(Ordering::Acquire) {
                pending = reader.changed.wait(pending).unwrap();
            }
            if reader.stop.load(Ordering::Acquire) {
                return;
            }
            pending.take().unwrap()
        };
        if let Some(task) = request.task.upgrade() {
            let path = task.view.lock().unwrap().ledger.clone();
            let rows = Ledger::read_only(&path)
                .and_then(|l| l.page(request.after, 96, request.failed_only));
            if task.page_token.load(Ordering::Acquire) != request.token {
                continue;
            }
            task.update(true, |v| {
                let (rows, error) = match rows {
                    Ok(rows) => (rows, None),
                    Err(error) if v.phase.terminal() => (vec![], Some(error.to_string().into())),
                    Err(_) => (vec![], None),
                };
                v.page = Some(Page {
                    after: request.after,
                    failed_only: request.failed_only,
                    rows: Arc::new(rows),
                    error,
                });
            });
        }
    }
}
pub fn error_label(code: i32) -> String {
    match code as u32 {
        0x80070005 => "无权访问".into(),
        0x80070002 | 0x80070003 => "源已不存在".into(),
        0x80070070 => "磁盘空间不足".into(),
        0x800700b7 | 0x80070050 => "此名称已存在".into(),
        0x80270027 | 0x80070020 => "文件正在使用".into(),
        0x80040201 => "源已改变，请重新选择".into(),
        0x80040202 => "目标已改变，请重新选择".into(),
        _ => windows::core::Error::from_hresult(windows::core::HRESULT(code)).message(),
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;
    #[test]
    fn native_rename_completes_with_durable_result_and_preserved_bytes() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "Backleaf-service-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("中文_👩🏽‍💻.txt");
        let content = b"native service identity fixture";
        std::fs::write(&source, content).unwrap();
        let expected = shell::identity::capture(&source).unwrap().identity;
        let (tx, _rx) = async_channel::bounded(1);
        let hub = Hub::new(root.join("state"), tx);
        let id = hub
            .direct(
                source.clone(),
                root.clone(),
                "renamed.txt".into(),
                Some(expected),
                false,
                0,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let view = loop {
            let view = hub.view(id).unwrap();
            if view.phase.terminal() {
                break view;
            }
            assert!(Instant::now() < deadline, "native service did not finish");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(view.phase, Phase::Complete, "{:?}", view.error);
        assert_eq!(view.successful, 1);
        assert_eq!(view.in_flight, 0);
        assert!(!source.exists());
        assert_eq!(std::fs::read(root.join("renamed.txt")).unwrap(), content);
        let ledger = Ledger::read_only(&view.ledger).unwrap();
        assert_eq!(ledger.counts().unwrap().successful, 1);
        drop(ledger);
        drop(hub);
        let resolved = root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(base)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-service-test-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn canceled_byte_delta_is_not_promoted_to_confirmed_data() {
        let mut progress = ByteProgress::default();
        progress.begin(true);
        progress.report(8 * 1024 * 1024);
        assert_eq!(progress.in_flight, 8 * 1024 * 1024);
        assert!(progress.end(None));
        assert_eq!(progress.completed, 0);
        assert_eq!(progress.in_flight, 0);
        progress.begin(true);
        progress.report(8 * 1024 * 1024 + 31);
        assert_eq!(progress.in_flight, 31);
        assert!(progress.end(Some(31)));
        assert_eq!(progress.completed, 31);
    }
}
