//! Bounded directory navigation reads, independent of the file-operation queue.
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub title: String,
    pub path: PathBuf,
    pub volume: bool,
}
#[derive(Clone, Debug)]
pub struct TreeRow {
    pub title: String,
    pub path: PathBuf,
    pub depth: usize,
    pub current: bool,
}
pub enum Notice {
    Common(Vec<Location>),
    Volumes(Result<Vec<Location>, String>),
    Clipboard(Result<(), String>),
    Tree {
        token: u64,
        rows: Vec<TreeRow>,
        truncated: bool,
        error: Option<String>,
    },
}
struct Shared {
    pending: Mutex<Option<(u64, PathBuf)>>,
    changed: Condvar,
    stop: AtomicBool,
    latest: std::sync::atomic::AtomicU64,
}
pub struct Worker(Arc<Shared>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_one();
    }
}
impl Worker {
    pub fn new(tx: async_channel::Sender<crate::model::Event>) -> Self {
        let shared = Arc::new(Shared {
            pending: Mutex::new(None),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            latest: std::sync::atomic::AtomicU64::new(0),
        });
        let state = shared.clone();
        thread::Builder::new()
            .name("backleaf-navigation".into())
            .spawn(move || {
                let _ = tx.send_blocking(crate::model::Event::Navigation(Notice::Common(
                    common_locations(),
                )));
                loop {
                    let request = {
                        let mut p = state.pending.lock().unwrap();
                        while p.is_none() && !state.stop.load(Ordering::Acquire) {
                            p = state.changed.wait(p).unwrap();
                        }
                        if state.stop.load(Ordering::Acquire) {
                            return;
                        }
                        p.take().unwrap()
                    };
                    let (token, path) = request;
                    let mut ancestors = path
                        .ancestors()
                        .filter(|p| !p.as_os_str().is_empty())
                        .map(PathBuf::from)
                        .collect::<Vec<_>>();
                    ancestors.reverse();
                    let mut rows = Vec::new();
                    let skip = ancestors.len().saturating_sub(12);
                    let mut bytes = 0;
                    for (i, p) in ancestors.into_iter().skip(skip).enumerate() {
                        bytes += p.as_os_str().len() * 2;
                        if bytes > 192 * 1024 {
                            break;
                        }
                        rows.push(TreeRow {
                            title: display_name(&p),
                            current: p == path,
                            path: p,
                            depth: i,
                        });
                    }
                    let depth = rows.len().min(5);
                    let mut truncated = false;
                    let mut error = None;
                    match std::fs::read_dir(&path) {
                        Ok(entries) => {
                            let mut children = Vec::new();
                            for item in entries {
                                if state.stop.load(Ordering::Acquire)
                                    || state.latest.load(Ordering::Acquire) != token
                                {
                                    break;
                                }
                                match item {
                                    Ok(entry) => {
                                        if entry.file_type().is_ok_and(|t| t.is_dir()) {
                                            let p = entry.path();
                                            let size = p.as_os_str().len() * 2;
                                            if children.len() >= 80 || bytes + size > 256 * 1024 {
                                                truncated = true;
                                                break;
                                            }
                                            bytes += size;
                                            children.push(TreeRow {
                                                title: entry
                                                    .file_name()
                                                    .to_string_lossy()
                                                    .into_owned(),
                                                path: p,
                                                depth,
                                                current: false,
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        error = Some(e.to_string());
                                        break;
                                    }
                                }
                            }
                            children.sort_by_cached_key(|r| r.title.to_lowercase());
                            rows.extend(children);
                        }
                        Err(e) => error = Some(e.to_string()),
                    }
                    if state.latest.load(Ordering::Acquire) == token
                        && tx
                            .send_blocking(crate::model::Event::Navigation(Notice::Tree {
                                token,
                                rows,
                                truncated,
                                error,
                            }))
                            .is_err()
                    {
                        return;
                    }
                }
            })
            .expect("navigation worker");
        Self(shared)
    }
    pub fn tree(&self, token: u64, path: PathBuf) {
        self.0.latest.store(token, Ordering::Release);
        *self.0.pending.lock().unwrap() = Some((token, path));
        self.0.changed.notify_one();
    }
}
pub fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| display_path(path))
}
pub fn display_path(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
}
/// Only a few bounded paths are copied; hidden ancestors remain reachable through the menu.
pub fn breadcrumbs(
    path: &std::path::Path,
    available: f32,
    measure: impl Fn(&str) -> f32,
) -> Vec<Option<PathBuf>> {
    let mut nearest = path
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .take(6)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    nearest.reverse();
    let total = nearest
        .iter()
        .map(|p| measure(&display_name(p)) + 8.)
        .sum::<f32>()
        + nearest.len().saturating_sub(1) as f32 * 16.
        + 24.;
    if nearest.len() <= 4 && total <= available {
        return nearest.into_iter().map(Some).collect();
    }
    let root = path.ancestors().last().unwrap_or(path).to_path_buf();
    let current = path.to_path_buf();
    if root == current {
        return vec![Some(current)];
    }
    let parent = path.parent().filter(|p| *p != root.as_path());
    if available < 220. {
        vec![None, Some(current)]
    } else if available < 400. || parent.is_none() {
        vec![Some(root), None, Some(current)]
    } else {
        vec![Some(root), None, parent.map(PathBuf::from), Some(current)]
    }
}
fn common_locations() -> Vec<Location> {
    use windows::{
        Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{
                FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music,
                FOLDERID_Pictures, FOLDERID_Profile, FOLDERID_Videos, KF_FLAG_DEFAULT,
                SHGetKnownFolderPath,
            },
        },
        core::GUID,
    };
    let entries: [(&str, GUID); 7] = [
        ("个人文件夹", FOLDERID_Profile),
        ("桌面", FOLDERID_Desktop),
        ("下载", FOLDERID_Downloads),
        ("文档", FOLDERID_Documents),
        ("图片", FOLDERID_Pictures),
        ("音乐", FOLDERID_Music),
        ("视频", FOLDERID_Videos),
    ];
    entries
        .into_iter()
        .filter_map(|(title, id)| unsafe {
            let raw = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None).ok()?;
            let result = raw.to_string().ok();
            CoTaskMemFree(Some(raw.0.cast()));
            result.map(|p| Location {
                title: title.into(),
                path: p.into(),
                volume: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_display_hides_internal_prefix_without_changing_navigation_targets() {
        let drive = PathBuf::from(r"\\?\C:\资料\当前");
        assert_eq!(display_path(&drive), r"C:\资料\当前");
        assert_eq!(display_name(std::path::Path::new(r"\\?\C:\")), r"C:\");
        assert_eq!(
            display_path(std::path::Path::new(r"\\?\UNC\server\share\中文")),
            r"\\server\share\中文"
        );
        assert_eq!(drive, PathBuf::from(r"\\?\C:\资料\当前"));
    }
    #[test]
    fn crumbs_preserve_unc_roots_unicode_and_exact_navigation_targets() {
        for raw in [
            r"C:\",
            r"C:\Users\例子\设计\最终 👩🏽‍💻",
            r"\\server\share\中文\文件夹\当前",
        ] {
            let path = PathBuf::from(raw);
            for width in [160., 320., 600.] {
                let crumbs = breadcrumbs(&path, width, |s| s.encode_utf16().count() as f32 * 13.);
                assert!(crumbs.len() <= 4);
                assert_eq!(crumbs.last().and_then(Option::as_ref), Some(&path));
                assert_eq!(
                    crumbs.iter().filter(|p| p.as_ref() == Some(&path)).count(),
                    1
                );
                assert!(crumbs.iter().flatten().all(|p| path.starts_with(p)));
                if width >= 220. {
                    assert_eq!(
                        crumbs
                            .first()
                            .and_then(Option::as_ref)
                            .map(PathBuf::as_path),
                        path.ancestors().last()
                    );
                }
            }
        }
    }
    #[test]
    fn directory_popup_is_bounded_and_later_request_wins() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "Backleaf-navigation-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        for i in 0..205 {
            std::fs::create_dir(path.join(format!("child-{i:03}"))).unwrap();
        }
        let (tx, rx) = async_channel::bounded(8);
        let worker = Worker::new(tx);
        worker.tree(1, base.clone());
        worker.tree(2, path.clone());
        loop {
            if let crate::model::Event::Navigation(Notice::Tree {
                token: 2,
                rows,
                truncated,
                error,
            }) = rx.recv_blocking().unwrap()
            {
                assert!(error.is_none());
                assert!(truncated);
                assert_eq!(
                    rows.iter()
                        .filter(|r| !r.current && r.path.parent() == Some(path.as_path()))
                        .count(),
                    80
                );
                assert!(rows.iter().any(|r| r.current && r.path == path));
                break;
            }
        }
        drop(worker);
        let resolved = path.canonicalize().unwrap();
        assert!(
            resolved.starts_with(base)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-navigation-test-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
}
