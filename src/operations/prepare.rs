//! Freeze names and identities into a durable ledger without touching the GPUI thread.
use super::{
    ledger::{Ledger, decode_path},
    windows::{self as shell, identity},
};
use crate::model::FrozenSelection;
use rusqlite::{Connection, OpenFlags};
use std::{
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub prepared: u64,
    pub total: u64,
    pub payload_bytes: u64,
    pub known_file_bytes: u64,
    pub has_directories: bool,
    pub complete: bool,
    pub canceled: bool,
    pub failed: u64,
}
pub struct Preparation {
    input: Connection,
    frozen: FrozenSelection,
    cursor: usize,
    progress: Progress,
    clipboard_limit: Option<u64>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, Source};
    use crate::selection::Selection;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        thread,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn source_lease_survives_navigation_and_preparation_does_not_take_new_files() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let directory = base.join(format!(
            "Backleaf-snapshot-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        assert!(directory.starts_with(&base));
        for n in 0..32 {
            fs::write(directory.join(format!("中文_{n:03}.txt")), b"data").unwrap();
        }
        let (tx, rx) = async_channel::bounded(8);
        let mut source = Source::directory(directory.clone(), 1, tx);
        while let Ok(event) = rx.recv_blocking() {
            if let Event::Count {
                count,
                done: true,
                error: None,
                ..
            } = event
            {
                if let Source::Directory { count: current, .. } = &mut source {
                    *current = count;
                }
                break;
            }
        }
        let mut selected = Selection::default();
        selected.all(source.count());
        let frozen = source.freeze_selection(selected.mask).unwrap();
        let snapshot = frozen.snapshot.database.clone();
        fs::write(directory.join("new-after-selection.txt"), b"not-selected").unwrap();
        drop(source);
        let ledger = Ledger::open(&directory.join("ledger.sqlite")).unwrap();
        let mut prepare = Preparation::new(frozen, None).unwrap();
        loop {
            if prepare.advance(&ledger, || false).unwrap().complete {
                break;
            }
        }
        assert_eq!(ledger.counts().unwrap().discovered, 32);
        assert!(
            !ledger
                .page(0, 96, false)
                .unwrap()
                .iter()
                .any(|r| r.path.ends_with("new-after-selection.txt"))
        );
        assert!(snapshot.exists());
        drop(prepare);
        drop(ledger);
        for _ in 0..100 {
            if !snapshot.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!snapshot.exists());
        assert!(
            directory.starts_with(&base)
                && directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-snapshot-test-")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
impl Preparation {
    pub fn new(frozen: FrozenSelection, clipboard_limit: Option<u64>) -> anyhow::Result<Self> {
        let input = Connection::open_with_flags(
            &frozen.snapshot.database,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        input.busy_timeout(Duration::from_secs(5))?;
        input.execute_batch("PRAGMA cache_size=-512")?;
        let total = frozen.mask.count_before(frozen.count) as u64;
        Ok(Self {
            input,
            frozen,
            cursor: 0,
            progress: Progress {
                total,
                payload_bytes: 22,
                ..Progress::default()
            },
            clipboard_limit,
        })
    }
    /// Stop between rows, and keep each preparation slice below both item and encoded-path limits.
    pub fn advance(
        &mut self,
        ledger: &Ledger,
        canceled: impl Fn() -> bool,
    ) -> anyhow::Result<Progress> {
        if self.progress.complete || self.progress.canceled {
            return Ok(self.progress.clone());
        }
        let started = Instant::now();
        let mut bytes = 0;
        let mut count = 0;
        ledger.begin()?;
        let parent = self
            .frozen
            .snapshot
            .parent
            .get()
            .ok_or_else(|| anyhow::anyhow!("Directory snapshot is not ready"))?;
        for id in self.frozen.mask.ids_from(self.cursor, self.frozen.count) {
            if canceled() {
                self.progress.canceled = true;
                break;
            }
            let (raw, expected): (Vec<u8>, Vec<u8>) = self.input.query_row(
                "SELECT raw,identity FROM files WHERE id=?1",
                [id as i64],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let name = decode_path(&raw);
            anyhow::ensure!(
                name.components().count() == 1 && !name.is_absolute(),
                "Invalid snapshot component"
            );
            let path: PathBuf = parent.join(name);
            let size = path.as_os_str().encode_wide().count() * 2;
            anyhow::ensure!(
                size <= shell::MAX_BATCH_PATH_BYTES,
                "Path exceeds preparation budget"
            );
            if bytes + size > shell::MAX_BATCH_PATH_BYTES {
                break;
            }
            let metadata = identity::capture(&path).and_then(|metadata| {
                if metadata.identity.encode().as_slice() == expected.as_slice() {
                    Ok(metadata)
                } else {
                    Err(windows::core::Error::new(
                        shell::SOURCE_ID_CHANGED,
                        "源文件已改变",
                    ))
                }
            });
            ledger.add_root(id as u64, &path, metadata.as_ref().ok())?;
            match metadata {
                Ok(metadata) => {
                    self.progress.has_directories |= metadata.directory;
                    self.progress.known_file_bytes = self
                        .progress
                        .known_file_bytes
                        .saturating_add(metadata.bytes.unwrap_or(0));
                }
                Err(error) => {
                    self.progress.failed += 1;
                    ledger.preparation_error(id as u64, error.code().0)?;
                    if self.clipboard_limit.is_some() {
                        ledger.commit()?;
                        return Err(anyhow::anyhow!("无法读取 {}：{}", path.display(), error));
                    }
                }
            }
            if let Some(limit) = self.clipboard_limit {
                let text = shell::clipboard_path(&path)?;
                self.progress.payload_bytes = self
                    .progress
                    .payload_bytes
                    .checked_add((text.len() * 2) as u64)
                    .ok_or_else(|| anyhow::anyhow!("Clipboard size overflow"))?;
                if self.progress.payload_bytes > limit {
                    ledger.commit()?;
                    anyhow::bail!("无法准备剪贴板，请减少选择后重试");
                }
            }
            self.progress.prepared += 1;
            self.cursor = id + 1;
            bytes += size;
            count += 1;
            if count >= shell::MAX_BATCH_ITEMS || started.elapsed() >= Duration::from_millis(50) {
                break;
            }
        }
        self.progress.complete = self
            .frozen
            .mask
            .ids_from(self.cursor, self.frozen.count)
            .next()
            .is_none();
        ledger.commit()?;
        Ok(self.progress.clone())
    }
}
#[cfg(test)]
mod replacement_tests {
    use super::*;
    #[test]
    fn replacing_selected_name_does_not_retarget_operation() {
        let f = crate::test_support::Fixture::new("replaced");
        std::fs::create_dir(f.0.join("source")).unwrap();
        let path = f.0.join("source/file.txt");
        std::fs::write(&path, b"original").unwrap();
        let frozen = f.freeze("source");
        std::fs::rename(&path, f.0.join("kept-original.txt")).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        let ledger = Ledger::open(&f.0.join("task.sqlite")).unwrap();
        let mut preparation = Preparation::new(frozen, None).unwrap();
        let p = preparation.advance(&ledger, || false).unwrap();
        assert!(p.complete);
        assert_eq!(p.failed, 1);
        assert!(ledger.identity(0).unwrap().is_none());
        let rows = ledger.page(0, 10, false).unwrap();
        assert_eq!(rows[0].hresult, Some(shell::SOURCE_ID_CHANGED.0));
        assert_eq!(std::fs::read(path).unwrap(), b"replacement");
    }
}
