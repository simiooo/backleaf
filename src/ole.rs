//! Native OLE sessions exchange immutable references; filesystem work stays off the GPUI thread.
use crate::{
    model::{Event, FrozenSelection},
    operations::{
        clipboard,
        ledger::Ledger,
        move_guard::{self, Target},
        prepare::Preparation,
        service::{Hub, Operation, Phase},
        windows::identity,
    },
};
use std::{
    collections::HashMap,
    ffi::c_void,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::HGLOBAL,
        System::Com::{CoCreateGuid, IDataObject},
    },
    core::Interface,
};

#[repr(C)]
struct Callbacks {
    offer: unsafe extern "C" fn(*mut c_void, *const u8, u64),
    import: unsafe extern "C" fn(*mut c_void, *mut c_void, u64),
    over: unsafe extern "C" fn(*mut c_void, i32, i32, u32, u32) -> u32,
    leave: unsafe extern "C" fn(*mut c_void),
    prepare: unsafe extern "C" fn(*mut c_void, u32) -> *mut c_void,
    execute: unsafe extern "C" fn(*mut c_void, *mut u32) -> i32,
    discard: unsafe extern "C" fn(*mut c_void),
    release: unsafe extern "C" fn(*mut c_void),
    ready: unsafe extern "C" fn(*mut c_void) -> i32,
}
unsafe extern "C" {
    fn backleaf_ole_install(
        owner: usize,
        context: *mut c_void,
        callbacks: Callbacks,
    ) -> *mut c_void;
    fn backleaf_ole_detach(site: *mut c_void);
    fn backleaf_ole_source(
        site: *mut c_void,
        context: *mut c_void,
        token: *const u8,
        render: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> i32,
        release: unsafe extern "C" fn(*mut c_void, i32, u32),
        cancel: unsafe extern "C" fn(*mut c_void),
    ) -> i32;
    fn backleaf_ole_source_busy(site: *mut c_void) -> i32;
}
pub struct Manifest {
    pub path: PathBuf,
    pub count: u64,
    pub bytes: u64,
    pub directories: bool,
    volume: Option<u64>,
}
impl Drop for Manifest {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
#[derive(Clone)]
enum Source {
    Frozen(FrozenSelection),
    Imported(Arc<Manifest>),
}
impl Source {
    fn count(&self) -> u64 {
        match self {
            Self::Frozen(s) => s.mask.count_before(s.count) as u64,
            Self::Imported(m) => m.count,
        }
    }
    fn volume(&self) -> Option<u64> {
        match self {
            Self::Frozen(s) => s
                .snapshot
                .parent_identity
                .get()
                .and_then(|b| identity::Identity::decode(b))
                .map(|i| i.volume),
            Self::Imported(m) => m.volume,
        }
    }
    fn visit(&self, mut f: impl FnMut(PathBuf) -> anyhow::Result<()>) -> anyhow::Result<()> {
        match self {
            Self::Frozen(s) => {
                let db = rusqlite::Connection::open_with_flags(
                    &s.snapshot.database,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                let parent = s
                    .snapshot
                    .parent
                    .get()
                    .ok_or_else(|| anyhow::anyhow!("源目录未就绪"))?;
                anyhow::ensure!(
                    s.snapshot.parent_identity.get()
                        == Some(&identity::capture(parent)?.identity.encode()),
                    "源目录已改变"
                );
                let mut q = db.prepare("SELECT raw,identity FROM files WHERE id=?1")?;
                for id in s.mask.ids_from(0, s.count) {
                    let (raw, expected): (Vec<u8>, Vec<u8>) =
                        q.query_row([id as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    let path = parent.join(crate::model::decode_name(&raw));
                    anyhow::ensure!(
                        identity::capture(&path)?.identity.encode().as_slice()
                            == expected.as_slice(),
                        "源文件已改变"
                    );
                    f(path)?;
                }
            }
            Self::Imported(m) => Ledger::read_only(&m.path)?.visit_roots(false, |r| {
                anyhow::ensure!(
                    r.identity.as_ref() == Some(&identity::capture(&r.path)?.identity),
                    "源文件已改变"
                );
                f(r.path)
            })?,
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum Destination {
    Folder(PathBuf),
    Pin(Option<String>, u64),
}
#[derive(Clone, Copy, Default)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub keys: u32,
    pub allowed: u32,
}
#[derive(Clone)]
struct Validated {
    generation: u64,
    version: u64,
    source: Source,
    target: Option<Target>,
    pin: Option<(PathBuf, identity::Identity, Option<String>, u64)>,
    volume: Option<u64>,
}
struct Pending {
    generation: u64,
    version: u64,
    source: Source,
    destination: Destination,
}
pub struct Shared {
    alive: AtomicBool,
    receiving: AtomicBool,
    generation: AtomicU64,
    version: AtomicU64,
    wake_pending: AtomicBool,
    pointer: Mutex<Pointer>,
    source: Mutex<Option<Source>>,
    destination: Mutex<Option<Destination>>,
    validated: Mutex<Option<Validated>>,
    error: Mutex<Option<String>>,
    pending: Mutex<Option<Pending>>,
    changed: Condvar,
    directory: PathBuf,
    hub: Hub,
    owner: usize,
    tx: async_channel::Sender<Event>,
}
impl Shared {
    fn wake(&self) {
        if !self.wake_pending.swap(true, Ordering::AcqRel)
            && self.tx.try_send(Event::OleChanged).is_err()
        {
            self.wake_pending.store(false, Ordering::Release);
        }
    }
    fn effect(&self, keys: u32, allowed: u32) -> u32 {
        let value = self.validated.lock().unwrap();
        let Some(v) = value.as_ref().filter(|v| {
            v.generation == self.generation.load(Ordering::Acquire)
                && v.version == self.version.load(Ordering::Acquire)
        }) else {
            return 0;
        };
        if v.pin.is_some() {
            return allowed & 1;
        }
        effect(keys, allowed, v.source.volume(), v.volume)
    }
    fn invalidate(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
        *self.validated.lock().unwrap() = None;
    }
}
pub fn effect(keys: u32, allowed: u32, source: Option<u64>, target: Option<u64>) -> u32 {
    let control = keys & 8 != 0;
    let shift = keys & 4 != 0;
    let desired = if control && shift {
        0
    } else if control {
        1
    } else if shift || source.is_some() && source == target {
        2
    } else {
        1
    };
    if desired & allowed != 0 { desired } else { 0 }
}
pub struct Site {
    native: *mut c_void,
    shared: Arc<Shared>,
}
impl Site {
    pub fn new(
        owner: usize,
        directory: PathBuf,
        hub: Hub,
        tx: async_channel::Sender<Event>,
        handle: gpui::AnyWindowHandle,
        cx: &mut gpui::App,
    ) -> anyhow::Result<Self> {
        let site = Self::native_site(owner, directory, hub, tx)?;
        let mut app = cx.to_async();
        READY.with(|callbacks| {
            callbacks.borrow_mut().insert(
                owner,
                Box::new(move |tick| {
                    handle
                        .update(&mut app, |view, window, cx| {
                            if tick && let Ok(root) = view.downcast::<crate::Backleaf>() {
                                root.update(cx, |root, cx| {
                                    root.sync_ole(window, cx);
                                    cx.notify();
                                });
                            }
                        })
                        .is_ok()
                }),
            );
        });
        Ok(site)
    }
    fn native_site(
        owner: usize,
        directory: PathBuf,
        hub: Hub,
        tx: async_channel::Sender<Event>,
    ) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared {
            alive: AtomicBool::new(true),
            receiving: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            version: AtomicU64::new(0),
            wake_pending: AtomicBool::new(false),
            pointer: Mutex::new(Pointer::default()),
            source: Mutex::new(None),
            destination: Mutex::new(None),
            validated: Mutex::new(None),
            error: Mutex::new(None),
            pending: Mutex::new(None),
            changed: Condvar::new(),
            directory,
            hub,
            owner,
            tx,
        });
        let context = Box::into_raw(Box::new(shared.clone())).cast();
        let native = unsafe {
            backleaf_ole_install(
                owner,
                context,
                Callbacks {
                    offer,
                    import: import_data,
                    over,
                    leave,
                    prepare,
                    execute,
                    discard,
                    release,
                    ready,
                },
            )
        };
        anyhow::ensure!(!native.is_null(), "无法注册Windows拖放");
        let site = Self { native, shared };
        let worker = site.shared.clone();
        std::thread::Builder::new()
            .name("backleaf-ole-target".into())
            .spawn(move || {
                while worker.alive.load(Ordering::Acquire) {
                    let request = {
                        let mut pending = worker.pending.lock().unwrap();
                        while pending.is_none() && worker.alive.load(Ordering::Acquire) {
                            pending = worker.changed.wait(pending).unwrap();
                        }
                        pending.take()
                    };
                    let Some(r) = request else { break };
                    let result = (|| -> anyhow::Result<Validated> {
                        let mut value = Validated {
                            generation: r.generation,
                            version: r.version,
                            source: r.source.clone(),
                            target: None,
                            pin: None,
                            volume: None,
                        };
                        if let Destination::Pin(before, revision) = r.destination {
                            anyhow::ensure!(r.source.count() == 1, "一次固定一个文件夹");
                            r.source.visit(|path| {
                                anyhow::ensure!(
                                    std::fs::metadata(&path)?.is_dir(),
                                    "只能固定文件夹"
                                );
                                let identity = identity::capture(&path)?.identity;
                                value.pin = Some((path, identity, before.clone(), revision));
                                Ok(())
                            })?;
                        } else if let Destination::Folder(path) = r.destination {
                            let target = Target::capture(path)?;
                            if r.source.count() <= 32 {
                                let chain = move_guard::ancestors(&target.path)?;
                                r.source.visit(|path| move_guard::check(&path, &chain))?;
                            }
                            value.volume = Some(target.resolved.volume);
                            value.target = Some(target);
                        }
                        Ok(value)
                    })();
                    if r.generation == worker.generation.load(Ordering::Acquire)
                        && r.version == worker.version.load(Ordering::Acquire)
                    {
                        match result {
                            Ok(v) => {
                                *worker.validated.lock().unwrap() = Some(v);
                                *worker.error.lock().unwrap() = None;
                            }
                            Err(e) => *worker.error.lock().unwrap() = Some(e.to_string()),
                        }
                        worker.wake();
                    }
                }
            })?;
        Ok(site)
    }
    pub fn changed(&self) {
        self.shared.wake_pending.store(false, Ordering::Release);
    }
    pub fn pointer(&self) -> Option<Pointer> {
        if !self.shared.receiving.load(Ordering::Acquire) {
            return None;
        }
        Some(*self.shared.pointer.lock().unwrap())
    }
    pub fn destination(&self, destination: Option<Destination>) {
        let mut current = self.shared.destination.lock().unwrap();
        if *current == destination {
            return;
        }
        crate::popup_menu::trace(match destination {
            None => "ole destination none",
            Some(Destination::Pin(_, _)) => "ole destination pin",
            Some(Destination::Folder(_)) => "ole destination folder",
        });
        *current = destination.clone();
        self.shared.invalidate();
        if let (Some(source), Some(destination)) =
            (self.shared.source.lock().unwrap().clone(), destination)
        {
            let version = self.shared.version.load(Ordering::Acquire);
            *self.shared.pending.lock().unwrap() = Some(Pending {
                generation: self.shared.generation.load(Ordering::Acquire),
                version,
                source,
                destination,
            });
            self.shared.changed.notify_one();
        }
    }
    pub fn label(&self) -> String {
        if let Some(e) = self.shared.error.lock().unwrap().clone() {
            return e;
        }
        if self.shared.destination.lock().unwrap().is_none() {
            return "不能放到此位置".into();
        }
        let pointer = *self.shared.pointer.lock().unwrap();
        let count = self
            .shared
            .source
            .lock()
            .unwrap()
            .as_ref()
            .map(Source::count)
            .unwrap_or(0);
        let effect = self.shared.effect(pointer.keys, pointer.allowed);
        let v = self.shared.validated.lock().unwrap();
        if let Some(v) = v.as_ref() {
            if v.pin.is_some() {
                return "固定到侧栏".into();
            }
            if effect == 0 {
                return "不能放到此位置".into();
            }
            return format!(
                "{} {} 项 → {}",
                if effect == 2 { "移动" } else { "复制" },
                count,
                crate::navigation::display_name(&v.target.as_ref().unwrap().path)
            );
        }
        "正在检查目标…".into()
    }
    pub fn busy(&self) -> bool {
        unsafe { backleaf_ole_source_busy(self.native) != 0 }
    }
    pub fn start(&self, selection: FrozenSelection) -> anyhow::Result<()> {
        crate::popup_menu::trace("ole start");
        anyhow::ensure!(
            registry().lock().unwrap().len() < 4,
            "仍有拖放在处理，请稍后重试"
        );
        let token = unsafe { CoCreateGuid() }?.to_u128().to_le_bytes();
        let state = Arc::new(Export {
            selection,
            directory: self.shared.directory.clone(),
            shared: self.shared.clone(),
            token,
            canceled: AtomicBool::new(false),
        });
        registry()
            .lock()
            .unwrap()
            .insert(token, Arc::downgrade(&state));
        let context = Box::into_raw(Box::new(state)).cast();
        let started = unsafe {
            backleaf_ole_source(
                self.native,
                context,
                token.as_ptr(),
                render_export,
                release_export,
                cancel_export,
            )
        };
        if started == 0 {
            unsafe { drop(Box::from_raw(context.cast::<Arc<Export>>())) };
            registry().lock().unwrap().remove(&token);
            anyhow::bail!("无法开始Windows拖放")
        }
        Ok(())
    }
}
impl Drop for Site {
    fn drop(&mut self) {
        READY.with(|callbacks| {
            callbacks.borrow_mut().remove(&self.shared.owner);
        });
        self.shared.alive.store(false, Ordering::Release);
        self.shared.changed.notify_all();
        unsafe { backleaf_ole_detach(self.native) }
    }
}
type ReadyCallback = Box<dyn FnMut(bool) -> bool>;
thread_local! {static READY:std::cell::RefCell<HashMap<usize,ReadyCallback>>=std::cell::RefCell::new(HashMap::new());}
unsafe extern "C" fn ready(context: *mut c_void) -> i32 {
    guarded(|| {
        let owner = unsafe { shared(context) }.owner;
        READY.with(|callbacks| {
            callbacks
                .try_borrow_mut()
                .ok()
                .is_some_and(|mut c| c.get_mut(&owner).is_some_and(|f| f(false)))
        }) as i32
    })
}
struct Export {
    selection: FrozenSelection,
    directory: PathBuf,
    shared: Arc<Shared>,
    token: [u8; 16],
    canceled: AtomicBool,
}
fn registry() -> &'static Mutex<HashMap<[u8; 16], Weak<Export>>> {
    static VALUE: OnceLock<Mutex<HashMap<[u8; 16], Weak<Export>>>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
fn unique(directory: &std::path::Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    Ok(directory.join(format!(
        "{:032x}.sqlite",
        unsafe { CoCreateGuid() }?.to_u128()
    )))
}
struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}
unsafe fn shared<'a>(context: *mut c_void) -> &'a Arc<Shared> {
    unsafe { &*context.cast::<Arc<Shared>>() }
}
fn guarded<T: Default>(body: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or_default()
}
unsafe extern "C" fn offer(context: *mut c_void, token: *const u8, generation: u64) {
    guarded(|| {
        let s = unsafe { shared(context) };
        crate::popup_menu::trace("ole offer");
        s.receiving.store(true, Ordering::Release);
        s.generation.store(generation, Ordering::Release);
        s.invalidate();
        *s.destination.lock().unwrap() = None;
        *s.error.lock().unwrap() = None;
        let token: [u8; 16] = unsafe { std::slice::from_raw_parts(token, 16) }
            .try_into()
            .unwrap();
        *s.source.lock().unwrap() = registry()
            .lock()
            .unwrap()
            .get(&token)
            .and_then(Weak::upgrade)
            .map(|e| Source::Frozen(e.selection.clone()));
        s.wake();
    })
}
static IMPORTS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn import_data(context: *mut c_void, object: *mut c_void, generation: u64) {
    guarded(|| {
        let s = unsafe { shared(context) };
        if s.source.lock().unwrap().is_some() {
            return;
        }
        if IMPORTS.fetch_add(1, Ordering::AcqRel) >= 4 {
            IMPORTS.fetch_sub(1, Ordering::AcqRel);
            return;
        }
        struct Permit;
        impl Drop for Permit {
            fn drop(&mut self) {
                IMPORTS.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let _permit = Permit;
        let result = (|| -> anyhow::Result<Manifest> {
            let path = unique(&s.directory)?;
            let mut temporary = Temporary(Some(path.clone()));
            let ledger = Ledger::open(&path)?;
            let object = unsafe { IDataObject::from_raw_borrowed(&object) }
                .ok_or_else(|| anyhow::anyhow!("拖放对象无效"))?;
            let (count, bytes, directories) = clipboard::import_object(object, &ledger, || {
                !s.alive.load(Ordering::Acquire)
                    || s.generation.load(Ordering::Acquire) != generation
            })?;
            let mut volume = None;
            let mut mixed = false;
            ledger.visit_roots(false, |root| {
                if let Some(id) = root.identity {
                    if volume.is_some_and(|v| v != id.volume) {
                        mixed = true
                    } else {
                        volume = Some(id.volume)
                    }
                }
                Ok(())
            })?;
            drop(ledger);
            temporary.0.take();
            Ok(Manifest {
                path,
                count,
                bytes,
                directories,
                volume: if mixed { None } else { volume },
            })
        })();
        if s.generation.load(Ordering::Acquire) == generation && s.alive.load(Ordering::Acquire) {
            match result {
                Ok(m) => *s.source.lock().unwrap() = Some(Source::Imported(Arc::new(m))),
                Err(e) => *s.error.lock().unwrap() = Some(e.to_string()),
            }
            *s.destination.lock().unwrap() = None;
            s.wake();
        }
    })
}
unsafe extern "C" fn over(context: *mut c_void, x: i32, y: i32, keys: u32, allowed: u32) -> u32 {
    guarded(|| {
        let s = unsafe { shared(context) };
        *s.pointer.lock().unwrap() = Pointer {
            x,
            y,
            keys,
            allowed,
        };
        s.wake();
        // The OLE modal loop can defer the normal GPUI executor wake. Resolve only
        // immutable viewport geometry through a guarded owner update; disk/identity
        // work remains on the target worker. A borrowed app simply defers this tick.
        READY.with(|callbacks| {
            if let Ok(mut c) = callbacks.try_borrow_mut()
                && let Some(f) = c.get_mut(&s.owner)
            {
                f(true);
            }
        });
        let effect = s.effect(keys, allowed);
        crate::popup_menu::trace(&format!(
            "ole over {x} {y} keys={keys} allowed={allowed} effect={effect}"
        ));
        effect
    })
}
unsafe extern "C" fn leave(context: *mut c_void) {
    guarded(|| {
        let s = unsafe { shared(context) };
        crate::popup_menu::trace("ole leave");
        s.receiving.store(false, Ordering::Release);
        s.generation.fetch_add(1, Ordering::AcqRel);
        s.invalidate();
        *s.source.lock().unwrap() = None;
        *s.destination.lock().unwrap() = None;
        s.wake();
    })
}
struct DropJob {
    shared: Arc<Shared>,
    validated: Validated,
    effect: u32,
}
unsafe extern "C" fn discard(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<DropJob>())) }
}
unsafe extern "C" fn prepare(context: *mut c_void, effect: u32) -> *mut c_void {
    guarded(|| {
        crate::popup_menu::trace(&format!("ole prepare {effect}"));
        let s = unsafe { shared(context) };
        let current = s.validated.lock().unwrap().clone();
        let Some(v) = current.filter(|v| {
            v.generation == s.generation.load(Ordering::Acquire)
                && v.version == s.version.load(Ordering::Acquire)
        }) else {
            return std::ptr::null_mut();
        };
        Box::into_raw(Box::new(DropJob {
            shared: s.clone(),
            validated: v,
            effect,
        }))
        .cast()
    })
}
unsafe extern "C" fn execute(context: *mut c_void, actual: *mut u32) -> i32 {
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<u32> {
            let job = unsafe { Box::from_raw(context.cast::<DropJob>()) };
            let source = job.validated.source;
            let shared = &job.shared;
            if let Some((path, expected, before, revision)) = job.validated.pin {
                anyhow::ensure!(
                    identity::capture(&path)?.identity == expected,
                    "源文件夹已改变"
                );
                shared.tx.send_blocking(Event::OlePin {
                    path,
                    before,
                    revision,
                })?;
                return Ok(1);
            }
            let target = job
                .validated
                .target
                .ok_or_else(|| anyhow::anyhow!("无效的目标"))?;
            target.verify()?;
            let operation = if job.effect == 2 {
                Operation::Move
            } else {
                Operation::Copy
            };
            let id = match source {
                Source::Frozen(s) => shared.hub.selection_guarded(
                    s,
                    operation,
                    target.path.clone(),
                    shared.owner,
                    Some(target),
                )?,
                Source::Imported(m) => {
                    shared
                        .hub
                        .dropped_manifest(m, operation, target, shared.owner)?
                }
            };
            let _ = shared.tx.send_blocking(Event::OleTask(id));
            loop {
                let view = shared
                    .hub
                    .view(id)
                    .ok_or_else(|| anyhow::anyhow!("任务不可用"))?;
                if view.phase.terminal() {
                    anyhow::ensure!(view.phase == Phase::Complete, "拖放未全部完成");
                    break;
                }
                if !shared.alive.load(Ordering::Acquire) {
                    shared.hub.cancel(id)
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(job.effect)
        }));
    match result {
        Ok(Ok(effect)) => {
            unsafe { *actual = effect };
            0
        }
        _ => {
            unsafe { *actual = 0 };
            0x80004005u32 as i32
        }
    }
}
unsafe extern "C" fn release(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Arc<Shared>>())) }
}
unsafe extern "C" fn render_export(context: *mut c_void, output: *mut *mut c_void) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<HGLOBAL> {
            let e = unsafe { &*context.cast::<Arc<Export>>() };
            let path = unique(&e.directory)?;
            let _temporary = Temporary(Some(path.clone()));
            let ledger = Ledger::open(&path)?;
            let mut prepare = Preparation::new(
                e.selection.clone(),
                Some(clipboard::MAX_PAYLOAD_BYTES as u64),
            )?;
            loop {
                let progress = prepare.advance(&ledger, || {
                    !e.shared.alive.load(Ordering::Acquire) || e.canceled.load(Ordering::Acquire)
                })?;
                anyhow::ensure!(!progress.canceled, "已取消");
                if progress.complete {
                    break;
                }
            }
            let result = clipboard::render_drop(&ledger, || {
                !e.shared.alive.load(Ordering::Acquire) || e.canceled.load(Ordering::Acquire)
            });
            drop(ledger);
            let _ = std::fs::remove_file(path);
            result
        },
    ));
    match result {
        Ok(Ok(h)) => {
            unsafe { *output = h.0 };
            0
        }
        _ => 0x80004005u32 as i32,
    }
}
unsafe extern "C" fn release_export(context: *mut c_void, result: i32, _effect: u32) {
    guarded(|| {
        crate::popup_menu::trace(&format!("ole source ended {result:x}"));
        let e = unsafe { Box::from_raw(context.cast::<Arc<Export>>()) };
        registry().lock().unwrap().remove(&e.token);
        let _ = e.shared.tx.send_blocking(Event::OleSourceEnded(result));
    })
}
unsafe extern "C" fn cancel_export(context: *mut c_void) {
    guarded(|| {
        unsafe { &*context.cast::<Arc<Export>>() }
            .canceled
            .store(true, Ordering::Release);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effect_matrix_respects_modifiers_offered_mask_and_volume() {
        for allowed in 0..4 {
            for (keys, source, target, want) in [
                (0, Some(1), Some(1), 2),
                (0, Some(1), Some(2), 1),
                (0, None, Some(1), 1),
                (8, Some(1), Some(1), 1),
                (4, Some(1), Some(2), 2),
                (12, Some(1), Some(1), 0),
            ] {
                assert_eq!(
                    effect(keys, allowed, source, target),
                    if allowed & want != 0 { want } else { 0 }
                );
            }
        }
    }
    #[test]
    fn imported_manifest_runs_native_copy_move_and_rejects_replacement_and_cycles() {
        use std::time::{Instant, SystemTime, UNIX_EPOCH};
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "Backleaf-drop-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        for name in ["source", "copy", "move"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        let source = root.join("source/资料_👩🏽‍💻.txt");
        let content = b"OLE manifest immutable identity";
        std::fs::write(&source, content).unwrap();
        fn manifest(root: &std::path::Path, path: &std::path::Path, name: &str) -> Arc<Manifest> {
            let file = root.join(format!("{name}.sqlite"));
            let ledger = Ledger::open(&file).unwrap();
            let m = identity::capture(path).unwrap();
            ledger.begin().unwrap();
            ledger.add_root(0, path, Some(&m)).unwrap();
            ledger.commit().unwrap();
            drop(ledger);
            Arc::new(Manifest {
                path: file,
                count: 1,
                bytes: m.bytes.unwrap_or(0),
                directories: m.directory,
                volume: Some(m.identity.volume),
            })
        }
        fn wait(hub: &Hub, id: u64) -> crate::operations::service::View {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let v = hub.view(id).unwrap();
                if v.phase.terminal() {
                    return v;
                }
                assert!(Instant::now() < deadline, "drop task timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let (tx, _rx) = async_channel::bounded(1);
        let hub = Hub::new(root.join("state"), tx);
        let imported = manifest(&root, &source, "files");
        let id = hub
            .dropped_manifest(
                imported.clone(),
                Operation::Copy,
                Target::capture(root.join("copy")).unwrap(),
                0,
            )
            .unwrap();
        let copied = wait(&hub, id);
        assert_eq!(copied.phase, Phase::Complete, "{:?}", copied.error);
        assert_eq!(copied.successful, 1);
        assert_eq!(
            std::fs::read(root.join("copy").join(source.file_name().unwrap())).unwrap(),
            content
        );
        assert!(source.exists());
        let id = hub
            .dropped_manifest(
                imported.clone(),
                Operation::Move,
                Target::capture(root.join("move")).unwrap(),
                0,
            )
            .unwrap();
        let moved = wait(&hub, id);
        assert_eq!(moved.phase, Phase::Complete, "{:?}", moved.error);
        assert!(!source.exists());
        assert_eq!(
            std::fs::read(root.join("move").join(source.file_name().unwrap())).unwrap(),
            content
        );
        drop(imported);
        let victim = root.join("source/replaced.txt");
        std::fs::write(&victim, b"original").unwrap();
        let stale = manifest(&root, &victim, "stale");
        std::fs::rename(&victim, root.join("source/old.txt")).unwrap();
        std::fs::write(&victim, b"replacement").unwrap();
        let id = hub
            .dropped_manifest(
                stale.clone(),
                Operation::Move,
                Target::capture(root.join("copy")).unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(wait(&hub, id).phase, Phase::Failed);
        assert_eq!(std::fs::read(&victim).unwrap(), b"replacement");
        assert!(!root.join("copy/replaced.txt").exists());
        drop(stale);
        let folder = root.join("source/folder");
        std::fs::create_dir_all(folder.join("child")).unwrap();
        let cyclic = manifest(&root, &folder, "cycle");
        let id = hub
            .dropped_manifest(
                cyclic.clone(),
                Operation::Move,
                Target::capture(folder.join("child")).unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(wait(&hub, id).phase, Phase::Failed);
        assert!(folder.join("child").is_dir());
        drop(cyclic);
        drop(hub);
        let resolved = root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(&base)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-drop-test-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
    unsafe extern "C" {
        fn backleaf_ole_probe_target(site: *mut c_void) -> *mut c_void;
        fn backleaf_ole_probe_step(
            target: *mut c_void,
            object: *mut c_void,
            step: u32,
            effect: *mut u32,
        ) -> i32;
        fn backleaf_ole_probe_release(target: *mut c_void);
        fn backleaf_ole_probe_pump();
    }
    #[test]
    fn native_target_negotiates_external_data_and_reports_completed_move_without_double_delete() {
        use windows::{
            Win32::{
                Foundation::HWND,
                System::{
                    Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
                    Ole::{CF_HDROP, OleInitialize, OleUninitialize},
                },
                UI::{
                    Shell::{IDataObjectAsyncCapability, SHCreateDataObject},
                    WindowsAndMessaging::*,
                },
            },
            core::w,
        };
        unsafe { OleInitialize(None) }.unwrap();
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { OleUninitialize() }
            }
        }
        let _apartment = Apartment;
        struct NativeWindow(HWND);
        impl Drop for NativeWindow {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let window = NativeWindow(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("Backleaf COM protocol test"),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    100,
                    100,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .unwrap(),
        );
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "Backleaf-com-drop-{}-{:032x}",
            std::process::id(),
            unsafe { CoCreateGuid() }.unwrap().to_u128()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        let (tx, _rx) = async_channel::unbounded();
        let (wake, _wake_rx) = async_channel::unbounded();
        let hub = Hub::new(root.join("state"), wake);
        let site =
            Site::native_site(window.0.0 as usize, root.join("imports"), hub.clone(), tx).unwrap();
        struct Probe(*mut c_void);
        impl Drop for Probe {
            fn drop(&mut self) {
                unsafe { backleaf_ole_probe_release(self.0) }
            }
        }
        let probe = Probe(unsafe { backleaf_ole_probe_target(site.native) });
        assert!(!probe.0.is_null());
        for asynchronous in [false, true] {
            let source = root.join(if asynchronous {
                "async.txt"
            } else {
                "sync.txt"
            });
            std::fs::write(&source, b"actual COM drop payload").unwrap();
            let ledger = Ledger::open(&root.join(if asynchronous {
                "async.sqlite"
            } else {
                "sync.sqlite"
            }))
            .unwrap();
            let metadata = identity::capture(&source).unwrap();
            ledger.begin().unwrap();
            ledger.add_root(0, &source, Some(&metadata)).unwrap();
            ledger.commit().unwrap();
            let object: IDataObject =
                unsafe { SHCreateDataObject(None, None, None::<&IDataObject>) }.unwrap();
            let capability = object.cast::<IDataObjectAsyncCapability>().unwrap();
            unsafe { capability.SetAsyncMode(asynchronous) }.unwrap();
            let h = clipboard::render_drop(&ledger, || false).unwrap();
            let f = FORMATETC {
                cfFormat: CF_HDROP.0,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            };
            let mut medium = STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            };
            medium.u.hGlobal = h;
            unsafe { object.SetData(&f, &medium, true) }.unwrap();
            let mut effect = 3;
            assert_eq!(
                unsafe { backleaf_ole_probe_step(probe.0, object.as_raw(), 0, &mut effect) },
                0
            );
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while site.shared.source.lock().unwrap().is_none() {
                assert!(std::time::Instant::now() < deadline);
                unsafe { backleaf_ole_probe_pump() };
                std::thread::sleep(Duration::from_millis(2));
            }
            site.destination(Some(Destination::Folder(root.join("target"))));
            while site.shared.effect(0, 3) != 2 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{:?}",
                    site.shared.error.lock().unwrap()
                );
                unsafe { backleaf_ole_probe_pump() };
                std::thread::sleep(Duration::from_millis(2));
            }
            effect = 3;
            assert_eq!(
                unsafe { backleaf_ole_probe_step(probe.0, object.as_raw(), 1, &mut effect) },
                0
            );
            assert_eq!(effect, 2);
            assert_eq!(
                unsafe { backleaf_ole_probe_step(probe.0, object.as_raw(), 2, &mut effect) },
                0
            );
            loop {
                unsafe { backleaf_ole_probe_pump() };
                if !unsafe { capability.InOperation() }.unwrap().as_bool()
                    && hub.views().last().is_some_and(|v| v.phase.terminal())
                {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(hub.views().last().unwrap().phase, Phase::Complete);
            assert!(!source.exists());
            assert_eq!(
                std::fs::read(root.join("target").join(source.file_name().unwrap())).unwrap(),
                b"actual COM drop payload"
            );
            // Optimized move: the target already moved the original; no second source deletion.
            let performed = unsafe {
                windows::Win32::System::DataExchange::RegisterClipboardFormatW(w!(
                    "Performed DropEffect"
                ))
            };
            let ff = FORMATETC {
                cfFormat: performed as u16,
                ..f
            };
            let mut report = unsafe { object.GetData(&ff) }.unwrap();
            let block = unsafe { report.u.hGlobal };
            let pointer =
                unsafe { windows::Win32::System::Memory::GlobalLock(block) } as *const u32;
            assert!(!pointer.is_null());
            assert_eq!(unsafe { pointer.read_unaligned() }, 0);
            unsafe {
                let _ = windows::Win32::System::Memory::GlobalUnlock(block);
                windows::Win32::System::Ole::ReleaseStgMedium(&mut report);
            }
            drop(ledger);
            drop(capability);
            drop(object);
        }
        drop(probe);
        drop(site);
        drop(hub);
        let resolved = root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(&base)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-com-drop-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
}
