//! Recursive Windows notifications. Buffer and mailbox are bounded; overflow requests reconciliation.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::Duration,
};
pub struct Change {
    pub root: i64,
    pub path: PathBuf,
}
pub struct Watch {
    stop: Arc<AtomicBool>,
    pub overflow: Arc<AtomicBool>,
    pub ready: Arc<AtomicBool>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Watch {
    pub fn new(root: i64, path: PathBuf, excluded: PathBuf, tx: SyncSender<Change>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let overflow = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let armed = ready.clone();
        let halted = stop.clone();
        let lost = overflow.clone();
        std::thread::Builder::new()
            .name("backleaf-index-watch".into())
            .spawn(move || {
                use std::os::windows::ffi::{OsStrExt, OsStringExt};
                use windows::{
                    Win32::{
                        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
                        Storage::FileSystem::*,
                        System::{
                            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
                            Threading::{CreateEventW, ResetEvent, WaitForSingleObject},
                        },
                    },
                    core::PCWSTR,
                };
                struct Handle(HANDLE);
                impl Drop for Handle {
                    fn drop(&mut self) {
                        unsafe {
                            let _ = CloseHandle(self.0);
                        }
                    }
                }
                let wide = path
                    .as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                while !halted.load(Ordering::Acquire) {
                    let result = (|| -> windows::core::Result<()> {
                        unsafe {
                            let directory = Handle(CreateFileW(
                                PCWSTR(wide.as_ptr()),
                                FILE_LIST_DIRECTORY.0,
                                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                None,
                                OPEN_EXISTING,
                                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                                None,
                            )?);
                            let original =
                                crate::operations::windows::identity::capture(&path)?.identity;
                            let signal = Handle(CreateEventW(None, true, false, None)?);
                            let mut buffer = vec![0u32; 15 * 1024];
                            let mut checked = std::time::Instant::now();
                            loop {
                                if halted.load(Ordering::Acquire) {
                                    return Ok(());
                                }
                                ResetEvent(signal.0)?;
                                let mut overlap = OVERLAPPED {
                                    hEvent: signal.0,
                                    ..Default::default()
                                };
                                ReadDirectoryChangesW(
                                    directory.0,
                                    buffer.as_mut_ptr().cast(),
                                    (buffer.len() * 4) as u32,
                                    true,
                                    FILE_NOTIFY_CHANGE_FILE_NAME
                                        | FILE_NOTIFY_CHANGE_DIR_NAME
                                        | FILE_NOTIFY_CHANGE_SIZE
                                        | FILE_NOTIFY_CHANGE_LAST_WRITE
                                        | FILE_NOTIFY_CHANGE_ATTRIBUTES
                                        | FILE_NOTIFY_CHANGE_SECURITY,
                                    None,
                                    Some(&mut overlap),
                                    None,
                                )?;
                                armed.store(true, Ordering::Release);
                                loop {
                                    if WaitForSingleObject(signal.0, 100) == WAIT_OBJECT_0 {
                                        break;
                                    }
                                    if checked.elapsed() >= Duration::from_secs(2) {
                                        checked = std::time::Instant::now();
                                        if !crate::operations::windows::identity::capture(&path)
                                            .is_ok_and(|m| m.identity == original)
                                        {
                                            lost.store(true, Ordering::Release);
                                            let _ = CancelIoEx(directory.0, Some(&overlap));
                                            let mut ignored = 0;
                                            let _ = GetOverlappedResult(
                                                directory.0,
                                                &overlap,
                                                &mut ignored,
                                                true,
                                            );
                                            return Err(windows::core::Error::new(
                                                windows::Win32::Foundation::E_FAIL,
                                                "索引位置已改变",
                                            ));
                                        }
                                    }
                                    if halted.load(Ordering::Acquire) {
                                        let _ = CancelIoEx(directory.0, Some(&overlap));
                                        let mut ignored = 0;
                                        let _ = GetOverlappedResult(
                                            directory.0,
                                            &overlap,
                                            &mut ignored,
                                            true,
                                        );
                                        return Ok(());
                                    }
                                }
                                let mut length = 0;
                                GetOverlappedResult(directory.0, &overlap, &mut length, false)?;
                                if length == 0 {
                                    lost.store(true, Ordering::Release);
                                    continue;
                                }
                                let bytes = std::slice::from_raw_parts(
                                    buffer.as_ptr().cast::<u8>(),
                                    length as usize,
                                );
                                let mut offset = 0usize;
                                loop {
                                    if offset + 12 > bytes.len() {
                                        lost.store(true, Ordering::Release);
                                        break;
                                    }
                                    let next = u32::from_le_bytes(
                                        bytes[offset..offset + 4].try_into().unwrap(),
                                    ) as usize;
                                    let n = u32::from_le_bytes(
                                        bytes[offset + 8..offset + 12].try_into().unwrap(),
                                    ) as usize;
                                    if !n.is_multiple_of(2) || offset + 12 + n > bytes.len() {
                                        lost.store(true, Ordering::Release);
                                        break;
                                    }
                                    let text = bytes[offset + 12..offset + 12 + n]
                                        .as_chunks::<2>()
                                        .0
                                        .iter()
                                        .map(|p| u16::from_le_bytes([p[0], p[1]]))
                                        .collect::<Vec<_>>();
                                    let relative =
                                        PathBuf::from(std::ffi::OsString::from_wide(&text));
                                    if relative
                                        .components()
                                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                                    {
                                        let full = path.join(relative);
                                        if !full.starts_with(&excluded)
                                            && !full.starts_with(
                                                std::env::temp_dir()
                                                    .canonicalize()
                                                    .unwrap_or_else(|_| std::env::temp_dir())
                                                    .join("Backleaf"),
                                            )
                                            && tx.try_send(Change { root, path: full }).is_err()
                                        {
                                            lost.store(true, Ordering::Release);
                                        }
                                    } else {
                                        lost.store(true, Ordering::Release);
                                    }
                                    if next == 0 {
                                        break;
                                    }
                                    if next < 12
                                        || !next.is_multiple_of(4)
                                        || offset + next <= offset
                                        || offset + next > bytes.len()
                                    {
                                        lost.store(true, Ordering::Release);
                                        break;
                                    }
                                    offset += next;
                                }
                            }
                        }
                    })();
                    if result.is_err() {
                        armed.store(false, Ordering::Release);
                        lost.store(true, Ordering::Release);
                        for _ in 0..20 {
                            if halted.load(Ordering::Acquire) {
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })
            .expect("recursive directory watcher");
        Self {
            stop,
            overflow,
            ready,
        }
    }
}
