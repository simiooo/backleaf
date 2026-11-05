//! Bounded location validation and drive discovery; no filesystem work on the UI thread.
use crate::{
    model::Event,
    navigation::{Location, Notice},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

static COPY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub fn path_copy_pending() -> bool {
    COPY_ACTIVE.load(Ordering::Acquire)
}
pub fn copy_path(path: PathBuf, tx: async_channel::Sender<Event>) -> Result<(), String> {
    if COPY_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return Err("正在复制路径，请稍后重试".into());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            COPY_ACTIVE.store(false, Ordering::Release);
        }
    }
    let sequence = crate::operations::clipboard::Clipboard::sequence();
    let spawned = thread::Builder::new()
        .name("backleaf-path-clipboard".into())
        .spawn(move || {
            let _guard = Guard;
            let result = crate::operations::clipboard::Clipboard::new()
                .and_then(|clipboard| clipboard.write_text(&clipboard_units(&path), sequence))
                .map_err(|error| {
                    let text = error.to_string();
                    if text.contains("剪贴板") {
                        text
                    } else {
                        "无法复制文件路径，请重试".into()
                    }
                });
            let _ = tx.send_blocking(Event::Navigation(Notice::Clipboard(result)));
        });
    if spawned.is_err() {
        COPY_ACTIVE.store(false, Ordering::Release);
        return Err("无法复制文件路径，请重试".into());
    }
    Ok(())
}
fn clipboard_units(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
    let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    if raw.starts_with(&unc) {
        [vec![92, 92], raw[unc.len()..].to_vec()].concat()
    } else if raw.starts_with(&prefix) {
        raw[prefix.len()..].to_vec()
    } else {
        raw
    }
}

struct ResolveRequest {
    token: u64,
    base: PathBuf,
    value: Option<String>,
}
struct ResolveState {
    pending: Mutex<Option<ResolveRequest>>,
    changed: Condvar,
    stop: AtomicBool,
}
pub struct Resolver(Arc<ResolveState>);
impl Resolver {
    pub fn new(tx: async_channel::Sender<Event>) -> Self {
        let state = Arc::new(ResolveState {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let shared = state.clone();
        thread::Builder::new()
            .name("backleaf-location".into())
            .spawn(move || {
                loop {
                    let request = {
                        let mut p = shared.pending.lock().unwrap();
                        while p.is_none() && !shared.stop.load(Ordering::Acquire) {
                            p = shared.changed.wait(p).unwrap();
                        }
                        if shared.stop.load(Ordering::Acquire) {
                            return;
                        }
                        p.take().unwrap()
                    };
                    let path = match request.value {
                        Some(value) => normalize_address(&request.base, &value),
                        None => Ok(request.base),
                    };
                    let value = path.and_then(|path| {
                        std::fs::read_dir(&path)
                            .map(|_| path)
                            .map_err(location_error)
                    });
                    if shared.stop.load(Ordering::Acquire)
                        || tx
                            .send_blocking(Event::PathResolved {
                                token: request.token,
                                value,
                            })
                            .is_err()
                    {
                        return;
                    }
                }
            })
            .expect("location worker");
        Self(state)
    }
    pub fn resolve(&self, token: u64, base: PathBuf, value: String) {
        *self.0.pending.lock().unwrap() = Some(ResolveRequest {
            token,
            base,
            value: Some(value),
        });
        self.0.changed.notify_one();
    }
    pub fn resolve_exact(&self, token: u64, path: PathBuf) {
        *self.0.pending.lock().unwrap() = Some(ResolveRequest {
            token,
            base: path,
            value: None,
        });
        self.0.changed.notify_one();
    }
}
impl Drop for Resolver {
    fn drop(&mut self) {
        let _guard = self.0.pending.lock().unwrap();
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_one();
    }
}

pub fn normalize_address(base: &Path, value: &str) -> Result<PathBuf, String> {
    let text = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        &value[1..value.len() - 1]
    } else {
        value
    };
    if text.trim().is_empty() {
        return Err("请输入位置".into());
    }
    if text.contains(['\0', '"', '\r', '\n']) {
        return Err("路径格式不正确".into());
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        if bytes.len() == 2 {
            return Ok(PathBuf::from(format!("{text}\\")));
        }
        if bytes[2] != b'\\' && bytes[2] != b'/' {
            return Err("请输入完整路径，如 D:\\文件夹".into());
        }
    }
    let path = PathBuf::from(text);
    Ok(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}
fn location_error(error: std::io::Error) -> String {
    if matches!(error.raw_os_error(), Some(15 | 21 | 55 | 1167)) {
        return "磁盘不可用，请检查连接".into();
    }
    match error.kind() {
        std::io::ErrorKind::NotFound => "找不到此位置",
        std::io::ErrorKind::PermissionDenied => "无权访问此位置",
        std::io::ErrorKind::NotADirectory => "此位置不是文件夹",
        std::io::ErrorKind::InvalidInput => "路径格式不正确",
        _ => "无法打开此位置",
    }
    .into()
}

pub use crate::volume_capacity::Volumes;
fn roots(mask: u32) -> impl Iterator<Item = PathBuf> {
    (0..26)
        .filter(move |bit| mask & (1 << bit) != 0)
        .map(|bit| PathBuf::from(format!("{}:\\", (b'A' + bit as u8) as char)))
}
pub(crate) fn read_volumes() -> Result<Vec<Location>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
        core::PCWSTR,
    };
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err("无法读取磁盘列表".into());
    }
    Ok(roots(mask)
        .map(|path| {
            let raw: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let kind = unsafe { GetDriveTypeW(PCWSTR(raw.as_ptr())) };
            let name = match kind {
                2 => "可移动磁盘",
                3 => "本地磁盘",
                4 => "网络磁盘",
                5 => "光驱",
                6 => "内存磁盘",
                _ => "磁盘",
            };
            Location {
                title: format!("{name} ({}:)", raw[0] as u8 as char),
                path,
                volume: true,
                entry: None,
                capacity: None,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_preserves_roots_unicode_and_explicit_drive_meaning() {
        let base = Path::new(r"D:\work\项目");
        for (value, expected) in [
            (r"C:", r"C:\"),
            (r"E:\书籍", r"E:\书籍"),
            (r"\temp", r"D:\temp"),
            (r"..\资料", r"D:\work\项目\..\资料"),
            (r#""\\server\share\中文""#, r"\\server\share\中文"),
        ] {
            assert_eq!(
                normalize_address(base, value).unwrap(),
                PathBuf::from(expected)
            );
        }
        for value in ["", "  ", "D:folder", "C:\0bad", "\"unbalanced", "a\nb"] {
            assert!(normalize_address(base, value).is_err(), "{value:?}");
        }
    }
    #[test]
    fn mask_has_only_assigned_drives_in_stable_order() {
        let rows: Vec<_> = roots((1 << 2) | (1 << 3) | (1 << 25) | (1 << 29)).collect();
        assert_eq!(
            rows,
            [
                PathBuf::from(r"C:\"),
                PathBuf::from(r"D:\"),
                PathBuf::from(r"Z:\")
            ]
        );
        assert_eq!(roots(0).count(), 0);
    }
    #[test]
    fn clipboard_copies_the_frozen_level_without_lossy_utf16_conversion() {
        use std::os::windows::ffi::OsStringExt;
        let full = PathBuf::from(r"\\?\C:\资料\项目");
        let parent = full.parent().unwrap();
        assert_eq!(
            String::from_utf16(&clipboard_units(parent)).unwrap(),
            r"C:\资料"
        );
        assert_eq!(
            String::from_utf16(&clipboard_units(Path::new(r"\\?\UNC\server\share\目录"))).unwrap(),
            r"\\server\share\目录"
        );
        let raw = [67, 58, 92, 0xd800, 65];
        assert_eq!(
            clipboard_units(&PathBuf::from(std::ffi::OsString::from_wide(&raw))),
            raw
        );
    }
    #[test]
    fn resolver_validates_real_directory_and_keeps_failed_value_out_of_navigation() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "backleaf-location-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("file.txt"), b"x").unwrap();
        let (tx, rx) = async_channel::bounded(8);
        let worker = Resolver::new(tx);
        for (token, value, ok) in [
            (71, path.to_string_lossy().into_owned(), true),
            (
                72,
                path.join("file.txt").to_string_lossy().into_owned(),
                false,
            ),
            (
                73,
                path.join("missing").to_string_lossy().into_owned(),
                false,
            ),
        ] {
            worker.resolve(token, base.clone(), value);
            let event = rx.recv_blocking().unwrap();
            let Event::PathResolved {
                token: actual,
                value,
            } = event
            else {
                panic!("unexpected event")
            };
            assert_eq!(actual, token);
            assert_eq!(value.is_ok(), ok);
        }
        drop(worker);
        drop(rx);
        let target = path.canonicalize().unwrap();
        assert!(target.starts_with(&base));
        assert_eq!(target.file_name(), path.file_name());
        std::fs::remove_dir_all(target).unwrap();
    }
}
