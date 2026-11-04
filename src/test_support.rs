use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
pub struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "Backleaf-test-{label}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn freeze(&self, name: &str) -> crate::model::FrozenSelection {
        use crate::model::{Event, Source};
        let (tx, rx) = async_channel::unbounded();
        let mut source = Source::directory(self.0.join(name), 1, tx);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(Instant::now() < deadline, "directory enumeration timed out");
            match rx.try_recv() {
                Ok(Event::Count {
                    count,
                    done: true,
                    error,
                    ..
                }) => {
                    assert!(error.is_none(), "{error:?}");
                    if let Source::Directory { count: current, .. } = &mut source {
                        *current = count;
                    }
                    let mut selection = crate::model::Selection::default();
                    selection.all(count);
                    return source.freeze_selection(selection.mask).unwrap();
                }
                Err(async_channel::TryRecvError::Closed) => panic!("enumerator disconnected"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0.parent() == Some(std::env::temp_dir().as_path())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("Backleaf-test-")
        );
        for _ in 0..40 {
            if std::fs::remove_dir_all(&self.0).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
