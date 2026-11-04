//! Bounded, level-triggered invalidation. Notifications never mutate a leased snapshot.
use crate::model::Event;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Monitor {
    stop: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Monitor {
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
    pub fn new(path: PathBuf, generation: u64, tx: async_channel::Sender<Event>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let epoch = Arc::new(AtomicU64::new(1));
        let halted = stop.clone();
        let revision = epoch.clone();
        std::thread::Builder::new()
            .name("backleaf-directory-watch".into())
            .spawn(move || {
                use std::os::windows::ffi::OsStrExt;
                use windows::{
                    Win32::{
                        Foundation::WAIT_OBJECT_0, Storage::FileSystem::*,
                        System::Threading::WaitForSingleObject,
                    },
                    core::PCWSTR,
                };
                struct Notification(windows::Win32::Foundation::HANDLE);
                impl Drop for Notification {
                    fn drop(&mut self) {
                        unsafe {
                            let _ = FindCloseChangeNotification(self.0);
                        }
                    }
                }
                let path = path.canonicalize().unwrap_or(path);
                let raw = path
                    .as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                let filter = FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_SECURITY;
                let mut handle = None;
                let mut dirty = false;
                let mut first = Instant::now();
                let mut last = first;
                let mut check = first;
                let mut retry = first - Duration::from_secs(3);
                let mut parent = crate::operations::windows::identity::capture(&path)
                    .ok()
                    .map(|m| m.identity);
                while !halted.load(Ordering::Acquire) && !tx.is_closed() {
                    if handle.is_none() && retry.elapsed() >= Duration::from_secs(2) {
                        retry = Instant::now();
                        handle = unsafe {
                            FindFirstChangeNotificationW(PCWSTR(raw.as_ptr()), false, filter)
                        }
                        .ok()
                        .map(Notification);
                    }
                    let signaled = handle
                        .as_ref()
                        .is_some_and(|h| unsafe { WaitForSingleObject(h.0, 100) == WAIT_OBJECT_0 });
                    if handle.is_none() {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    let mut changed = signaled;
                    if signaled
                        && handle
                            .as_ref()
                            .is_some_and(|h| unsafe { FindNextChangeNotification(h.0) }.is_err())
                    {
                        handle = None;
                    }
                    // The notification object follows an opened directory, not its pathname.
                    // Re-check path identity and reconcile remote/unsupported notifications periodically.
                    if check.elapsed() >= Duration::from_secs(2) {
                        check = Instant::now();
                        let now = crate::operations::windows::identity::capture(&path)
                            .ok()
                            .map(|m| m.identity);
                        if now != parent {
                            parent = now;
                            handle = None;
                            changed = true;
                        }
                        if handle.is_none() || remote_path(&path) {
                            changed = true;
                        }
                    }
                    if changed {
                        revision.fetch_add(1, Ordering::AcqRel);
                        if !dirty {
                            first = Instant::now();
                            dirty = true;
                        }
                        last = Instant::now();
                    }
                    if dirty
                        && (last.elapsed() >= Duration::from_millis(350)
                            || first.elapsed() >= Duration::from_secs(2))
                        && tx
                            .try_send(Event::DirectoryDirty {
                                generation,
                                epoch: revision.load(Ordering::Acquire),
                            })
                            .is_ok()
                    {
                        dirty = false;
                    }
                }
            })
            .expect("directory watcher");
        Self { stop, epoch }
    }
}

fn remote_path(path: &std::path::Path) -> bool {
    use std::path::{Component, Prefix};
    matches!(
        path.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_fallback_distinguishes_dos_namespace_from_network() {
        for path in [r"C:\files", r"\\?\C:\files"] {
            assert!(!remote_path(std::path::Path::new(path)));
        }
        for path in [r"\\server\share\files", r"\\?\UNC\server\share\files"] {
            assert!(remote_path(std::path::Path::new(path)));
        }
    }
}
#[cfg(test)]
mod notification_tests {
    use super::*;
    #[test]
    fn external_file_changes_emit_a_coalesced_generation_invalidation() {
        let f = crate::test_support::Fixture::new("watch");
        let (tx, rx) = async_channel::unbounded();
        let monitor = Monitor::new(f.0.clone(), 77, tx);
        std::thread::sleep(Duration::from_millis(150));
        for i in 0..25 {
            std::fs::write(f.0.join(format!("external-{i}.txt")), b"outside").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            match rx.try_recv() {
                Ok(Event::DirectoryDirty { generation, epoch }) => {
                    assert_eq!(generation, 77);
                    assert!(epoch > 1);
                    assert!(monitor.epoch() >= epoch);
                    break;
                }
                _ => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}
