//! Thin Shell adapter. COM objects never leave the caller's STA.
//! The observer must persist results and coalesce cosmetic progress outside this layer.
#![allow(non_snake_case)]
#[path = "identity.rs"]
pub mod identity;
#[path = "names.rs"]
pub mod names;
use std::{
    ffi::OsString,
    marker::PhantomData,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use windows::{
    Win32::{
        Foundation::{E_ABORT, E_INVALIDARG, E_NOTIMPL, HWND},
        Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            PropertiesSystem::{PDOPS_CANCELLED, PDOPS_RUNNING, PDOPSTATUS},
            *,
        },
    },
    core::{Error, HRESULT, PCWSTR, Ref, Result, implement},
};

pub const MAX_BATCH_ITEMS: usize = 256;
pub const MAX_BATCH_PATH_BYTES: usize = 256 * 1024;
pub const SOURCE_ID_CHANGED: HRESULT = HRESULT(0x80040201_u32 as i32);
pub const TARGET_ID_CHANGED: HRESULT = HRESULT(0x80040202_u32 as i32);

/// !Send and !Sync: an operation and its callbacks belong to this apartment.
pub struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Copy,
    Move,
    Recycle,
    Rename,
    NewFolder,
}
#[derive(Clone, Copy, Debug)]
pub enum Collision {
    Ask,
    KeepBoth,
    #[allow(
        dead_code,
        reason = "Explicit overwrite is exercised only by the isolated Shell probe; UI uses native conflict decisions"
    )]
    Replace,
}
#[derive(Clone, Debug)]
pub struct Step {
    pub key: u64,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub new_name: Option<OsString>,
    pub expected: Option<identity::Identity>,
    pub expected_destination: Option<identity::Identity>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Notice {
    DialogStarted,
    DialogStopped,
    Progress {
        points: u64,
        points_total: u64,
        bytes: u64,
        bytes_total: u64,
        items: u64,
        items_total: u64,
    },
    SinkWork {
        done: u32,
        total: u32,
    },
    Before {
        key: u64,
        source: Option<PathBuf>,
        flags: u32,
        root: bool,
    },
    After {
        key: u64,
        source: Option<PathBuf>,
        created: Option<PathBuf>,
        hresult: i32,
        root: bool,
    },
    Finished {
        i32_hresult: i32,
    },
}
pub trait Observer: Send + Sync {
    fn notice(&self, notice: Notice);
    fn canceled(&self) -> bool;
}
#[derive(Debug, serde::Serialize)]
pub struct Outcome {
    pub perform_hresult: i32,
    pub aborted: bool,
    pub queued: usize,
    pub queue_errors: Vec<(u64, i32)>,
}

/// Shell parsing rejects the verbatim prefix produced by Rust canonicalize on this Windows build.
/// Strip only DOS/UNC prefixes; reject names whose Win32 normalization would change their identity.
fn parsing_name(path: &Path) -> Result<Vec<u16>> {
    if !path.is_absolute() {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let raw = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let prefix = |s: &str| raw.starts_with(&s.encode_utf16().collect::<Vec<_>>());
    let value = if prefix("\\\\?\\UNC\\") {
        [vec![b'\\' as u16, b'\\' as u16], raw[8..].to_vec()].concat()
    } else if prefix("\\\\?\\") && raw.len() > 6 && raw[5] == b':' as u16 {
        raw[4..].to_vec()
    } else if prefix("\\\\?\\") || prefix("\\\\.\\") {
        return Err(Error::from_hresult(E_INVALIDARG));
    } else {
        raw
    };
    if value.contains(&0)
        || value
            .split(|c| *c == b'\\' as u16 || *c == b'/' as u16)
            .any(|s| {
                s.last()
                    .is_some_and(|c| *c == b'.' as u16 || *c == b' ' as u16)
            })
    {
        return Err(Error::new(
            E_INVALIDARG,
            "Shell cannot safely address this path",
        ));
    }
    // Unlike CreateFileW, the Shell parser rejects ordinary DOS paths with '/'.
    // Identity is checked again after parsing, so this never silently changes a target.
    Ok(value
        .into_iter()
        .map(|c| if c == b'/' as u16 { b'\\' as u16 } else { c })
        .chain(Some(0))
        .collect())
}
/// Standard CF_HDROP paths use the same spelling as Shell items; the returned path includes NUL.
pub fn clipboard_path(path: &Path) -> Result<Vec<u16>> {
    parsing_name(path)
}
fn name_wide(name: Option<&OsString>) -> Option<Vec<u16>> {
    name.map(|n| n.encode_wide().chain(Some(0)).collect())
}
fn item(path: &Path) -> Result<IShellItem> {
    let original = identity::capture(path)?.identity;
    let text = parsing_name(path)?;
    let object: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(text.as_ptr()), None) }?;
    let resolved = path_of(Some(&object)).ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
    if identity::capture(&resolved)?.identity != original {
        return Err(Error::new(
            SOURCE_ID_CHANGED,
            "Shell path resolves to a different file",
        ));
    }
    Ok(object)
}
fn path_of(value: Option<&IShellItem>) -> Option<PathBuf> {
    let value = value?;
    unsafe {
        let text = value.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = PathBuf::from(OsString::from_wide(text.as_wide()));
        CoTaskMemFree(Some(text.0.cast()));
        Some(path)
    }
}
fn check(observer: &dyn Observer) -> Result<()> {
    if observer.canceled() {
        Err(Error::from_hresult(E_ABORT))
    } else {
        Ok(())
    }
}

/// A single bounded batch. The higher layer owns identity checks, decisions and durable manifests.
/// Ask preserves native collision handling for races; Replace may only follow an explicit decision.
pub fn perform(
    _apartment: &Apartment,
    kind: Kind,
    steps: &[Step],
    collision: Collision,
    owner: usize,
    observer: Arc<dyn Observer>,
) -> Result<Outcome> {
    if steps.len() > MAX_BATCH_ITEMS
        || matches!((kind, collision), (Kind::Recycle, Collision::Replace))
    {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let encoded_bytes: usize = steps
        .iter()
        .map(|s| {
            s.source.as_os_str().encode_wide().count() * 2
                + s.destination.as_os_str().encode_wide().count() * 2
                + s.new_name
                    .as_ref()
                    .map_or(0, |s| s.encode_wide().count() * 2)
        })
        .sum();
    if encoded_bytes > MAX_BATCH_PATH_BYTES {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    check(observer.as_ref())?;
    unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
        let mut flags =
            FOF_NOERRORUI | FOF_NOCONFIRMMKDIR | FOF_NO_CONNECTED_ELEMENTS | FOFX_ADDUNDORECORD;
        if matches!(kind, Kind::Recycle) {
            flags |= FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING | FOFX_EARLYFAILURE;
        }
        match collision {
            Collision::Ask => {}
            Collision::KeepBoth => flags |= FOF_RENAMEONCOLLISION | FOFX_PRESERVEFILEEXTENSIONS,
            Collision::Replace => flags |= FOF_NOCONFIRMATION,
        }
        operation.SetOperationFlags(flags)?;
        operation.SetOwnerWindow(HWND(owner as *mut _))?;
        let dialog: IOperationsProgressDialog = ProgressDialog {
            observer: observer.clone(),
        }
        .into();
        operation.SetProgressDialog(&dialog)?;
        let global: IFileOperationProgressSink = Sink {
            key: u64::MAX,
            observer: observer.clone(),
            recycle_only: matches!(kind, Kind::Recycle),
            guard: None,
            destination_guard: None,
        }
        .into();
        let cookie = operation.Advise(&global)?;
        let mut queued = 0;
        let mut queue_errors = Vec::new();
        for step in steps {
            if observer.canceled() {
                break;
            }
            let result = (|| -> Result<()> {
                if let Some(name) = &step.new_name {
                    let text = name
                        .to_str()
                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                    names::validate(text).map_err(|why| Error::new(E_INVALIDARG, why))?;
                }
                if matches!(kind, Kind::Rename | Kind::NewFolder) && step.new_name.is_none() {
                    return Err(Error::from_hresult(E_INVALIDARG));
                }
                let root = if matches!(kind, Kind::NewFolder) {
                    &step.destination
                } else {
                    &step.source
                };
                let expected = identity::capture(root)?.identity;
                if step.expected.as_ref().is_some_and(|id| *id != expected) {
                    return Err(Error::new(
                        SOURCE_ID_CHANGED,
                        "Prepared file identity changed",
                    ));
                }
                let source = if matches!(kind, Kind::NewFolder) {
                    None
                } else {
                    Some(item(&step.source)?)
                };
                let destination = if matches!(kind, Kind::Copy | Kind::Move | Kind::NewFolder) {
                    Some(item(&step.destination)?)
                } else {
                    None
                };
                if let Some(expected) = &step.expected_destination
                    && identity::capture(&step.destination)?.identity != *expected
                {
                    return Err(Error::new(
                        TARGET_ID_CHANGED,
                        "Destination identity changed",
                    ));
                }
                let parsed = path_of(if matches!(kind, Kind::NewFolder) {
                    destination.as_ref()
                } else {
                    source.as_ref()
                })
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                let sink: IFileOperationProgressSink = Sink {
                    key: step.key,
                    observer: observer.clone(),
                    recycle_only: matches!(kind, Kind::Recycle),
                    guard: Some((root.clone(), parsed, expected)),
                    destination_guard: step
                        .expected_destination
                        .clone()
                        .map(|id| (step.destination.clone(), id)),
                }
                .into();
                let name = name_wide(step.new_name.as_ref());
                let name = name.as_ref().map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr()));
                match kind {
                    Kind::Copy => operation.CopyItem(
                        source.as_ref().unwrap(),
                        destination.as_ref().unwrap(),
                        name,
                        &sink,
                    ),
                    Kind::Move => operation.MoveItem(
                        source.as_ref().unwrap(),
                        destination.as_ref().unwrap(),
                        name,
                        &sink,
                    ),
                    Kind::Recycle => operation.DeleteItem(source.as_ref().unwrap(), &sink),
                    Kind::Rename => operation.RenameItem(source.as_ref().unwrap(), name, &sink),
                    Kind::NewFolder => operation.NewItem(
                        destination.as_ref().unwrap(),
                        FILE_ATTRIBUTE_DIRECTORY.0,
                        name,
                        PCWSTR::null(),
                        &sink,
                    ),
                }
            })();
            match result {
                Ok(()) => queued += 1,
                Err(error) => queue_errors.push((step.key, error.code().0)),
            }
        }
        let result = if observer.canceled() {
            Err(Error::from_hresult(E_ABORT))
        } else if queued > 0 {
            operation.PerformOperations()
        } else {
            Ok(())
        };
        let aborted = operation.GetAnyOperationsAborted()?.as_bool() || observer.canceled();
        operation.Unadvise(cookie)?;
        Ok(Outcome {
            perform_hresult: result.err().map_or(0, |e| e.code().0),
            aborted,
            queued,
            queue_errors,
        })
    }
}

#[implement(IOperationsProgressDialog)]
struct ProgressDialog {
    observer: Arc<dyn Observer>,
}
impl IOperationsProgressDialog_Impl for ProgressDialog_Impl {
    fn StartProgressDialog(&self, _owner: HWND, _flags: u32) -> Result<()> {
        self.observer.notice(Notice::DialogStarted);
        check(self.observer.as_ref())
    }
    fn StopProgressDialog(&self) -> Result<()> {
        self.observer.notice(Notice::DialogStopped);
        Ok(())
    }
    fn SetOperation(&self, _action: SPACTION) -> Result<()> {
        Ok(())
    }
    fn SetMode(&self, _mode: u32) -> Result<()> {
        Ok(())
    }
    fn UpdateProgress(
        &self,
        points: u64,
        points_total: u64,
        bytes: u64,
        bytes_total: u64,
        items: u64,
        items_total: u64,
    ) -> Result<()> {
        self.observer.notice(Notice::Progress {
            points,
            points_total,
            bytes,
            bytes_total,
            items,
            items_total,
        });
        check(self.observer.as_ref())
    }
    fn UpdateLocations(
        &self,
        _source: Ref<IShellItem>,
        _target: Ref<IShellItem>,
        _item: Ref<IShellItem>,
    ) -> Result<()> {
        check(self.observer.as_ref())
    }
    fn ResetTimer(&self) -> Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> Result<()> {
        Ok(())
    }
    fn GetMilliseconds(&self, elapsed: *mut u64, remaining: *mut u64) -> Result<()> {
        unsafe {
            if !elapsed.is_null() {
                *elapsed = 0;
            }
            if !remaining.is_null() {
                *remaining = 0;
            }
        }
        Err(Error::from_hresult(E_NOTIMPL))
    }
    fn GetOperationStatus(&self) -> Result<PDOPSTATUS> {
        Ok(if self.observer.canceled() {
            PDOPS_CANCELLED
        } else {
            PDOPS_RUNNING
        })
    }
}

#[implement(IFileOperationProgressSink)]
struct Sink {
    key: u64,
    observer: Arc<dyn Observer>,
    recycle_only: bool,
    guard: Option<(PathBuf, PathBuf, identity::Identity)>,
    destination_guard: Option<(PathBuf, identity::Identity)>,
}
impl Sink {
    fn before(&self, flags: u32, source: Ref<IShellItem>) -> Result<()> {
        if let Some((path, expected)) = &self.destination_guard
            && identity::capture(path)?.identity != *expected
        {
            return Err(Error::new(
                TARGET_ID_CHANGED,
                "Destination changed before execution",
            ));
        }
        let path = path_of(source.as_ref());
        if let Some((original, parsed, expected)) = &self.guard
            && path.as_ref() == Some(parsed)
            && identity::capture(original)?.identity != *expected
        {
            return Err(Error::new(
                SOURCE_ID_CHANGED,
                "File identity changed before execution",
            ));
        }
        let root = self
            .guard
            .as_ref()
            .is_some_and(|g| path.as_ref() == Some(&g.1));
        if self.key != u64::MAX {
            self.observer.notice(Notice::Before {
                key: self.key,
                source: path,
                flags,
                root,
            });
        }
        check(self.observer.as_ref())
    }
    fn after(
        &self,
        source: Ref<IShellItem>,
        created: Ref<IShellItem>,
        result: HRESULT,
    ) -> Result<()> {
        let source = path_of(source.as_ref());
        let root = self
            .guard
            .as_ref()
            .is_some_and(|g| source.as_ref() == Some(&g.1));
        if self.key != u64::MAX {
            self.observer.notice(Notice::After {
                key: self.key,
                source,
                created: path_of(created.as_ref()),
                hresult: result.0,
                root,
            });
        }
        // A result must be recorded even when cancellation was already requested.
        Ok(())
    }
}
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn StartOperations(&self) -> Result<()> {
        check(self.observer.as_ref())
    }
    fn FinishOperations(&self, result: HRESULT) -> Result<()> {
        if self.key == u64::MAX {
            self.observer.notice(Notice::Finished {
                i32_hresult: result.0,
            });
        }
        Ok(())
    }
    fn PreRenameItem(&self, flags: u32, item: Ref<IShellItem>, _name: &PCWSTR) -> Result<()> {
        self.before(flags, item)
    }
    fn PostRenameItem(
        &self,
        _flags: u32,
        item: Ref<IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        self.after(item, created, result)
    }
    fn PreMoveItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        _dest: Ref<IShellItem>,
        _name: &PCWSTR,
    ) -> Result<()> {
        self.before(flags, item)
    }
    fn PostMoveItem(
        &self,
        _flags: u32,
        item: Ref<IShellItem>,
        _dest: Ref<IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        self.after(item, created, result)
    }
    fn PreCopyItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        _dest: Ref<IShellItem>,
        _name: &PCWSTR,
    ) -> Result<()> {
        self.before(flags, item)
    }
    fn PostCopyItem(
        &self,
        _flags: u32,
        item: Ref<IShellItem>,
        _dest: Ref<IShellItem>,
        _name: &PCWSTR,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        self.after(item, created, result)
    }
    fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> Result<()> {
        if self.recycle_only && flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            return Err(Error::from_hresult(E_ABORT));
        }
        self.before(flags, item)
    }
    fn PostDeleteItem(
        &self,
        _flags: u32,
        item: Ref<IShellItem>,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        self.after(item, created, result)
    }
    fn PreNewItem(&self, flags: u32, dest: Ref<IShellItem>, _name: &PCWSTR) -> Result<()> {
        self.before(flags, dest)
    }
    fn PostNewItem(
        &self,
        _flags: u32,
        dest: Ref<IShellItem>,
        _name: &PCWSTR,
        _template: &PCWSTR,
        _attrs: u32,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> Result<()> {
        self.after(dest, created, result)
    }
    fn UpdateProgress(&self, total: u32, done: u32) -> Result<()> {
        if self.key == u64::MAX {
            self.observer.notice(Notice::SinkWork { done, total });
        }
        check(self.observer.as_ref())
    }
    fn ResetTimer(&self) -> Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> Result<()> {
        Ok(())
    }
}
