//! Ignored real-file benchmark/profile for index maintenance. It separates the
//! costs that decide the next optimization: directory enumeration, per-file
//! identity capture, upsert/SQL+triggers, startup re-scan, and notification-style
//! incremental updates. Run with:
//! `cargo test --release --locked -p backleaf maintenance_benchmark:: -- --ignored --nocapture`
use crate::index::{self, Action, Service, Status};
use crate::model::Event;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SCOPE: &str = "Real owned files on one NTFS tree, release build, OS cache warm after fixture creation. Maintenance worker/DB latency only: no UI, GPU, cold-disk or power claim. Profile counters are #[cfg(test)] instrumentation inside reconcile/upsert/incremental.";

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

struct Fixture {
    _fixture: crate::test_support::Fixture,
    root: PathBuf,
    state: PathBuf,
    files: usize,
    dirs: usize,
}

fn create_fixture(files: usize, dirs: usize) -> Fixture {
    let fixture = crate::test_support::Fixture::new("index-maintenance-benchmark");
    let root = fixture.0.join("files");
    std::fs::create_dir(&root).unwrap();
    let mut dir_paths = Vec::with_capacity(dirs);
    for dir in 0..dirs {
        let path = root.join(format!("d{dir:04}"));
        std::fs::create_dir(&path).unwrap();
        dir_paths.push(path);
    }
    for i in 0..files {
        std::fs::write(
            dir_paths[i % dirs].join(format!("doc-{i:06}.txt")),
            b"maintenance benchmark",
        )
        .unwrap();
    }
    let state = fixture.0.join("state");
    Fixture {
        _fixture: fixture,
        root,
        state,
        files,
        dirs,
    }
}

fn enumerate_tree(root: &Path) -> (usize, Vec<PathBuf>) {
    let mut count = 0;
    let mut files = Vec::new();
    let mut frontier = vec![root.to_owned()];
    while let Some(directory) = frontier.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            count += 1;
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                frontier.push(path);
            } else {
                files.push(path);
            }
        }
    }
    (count, files)
}

fn file_bytes(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn wal_bytes(state: &Path) -> u64 {
    file_bytes(&state.join("file-index.sqlite-wal"))
}

fn profile_json(prefix: &str) -> serde_json::Value {
    let enumerate = index::profile::millis(&index::profile::ENUMERATE_NS);
    let identity = index::profile::millis(&index::profile::IDENTITY_NS);
    let upsert = index::profile::millis(&index::profile::UPSERT_NS);
    serde_json::json!({
        format!("{prefix}enumerate_ms"): rounded(enumerate),
        format!("{prefix}identity_ms"): rounded(identity),
        format!("{prefix}upsert_ms"): rounded(upsert),
        format!("{prefix}entries"): index::profile::entries(),
    })
}

fn rounded(value: f64) -> f64 {
    (value * 1000.).round() / 1000.
}

fn wait_status(
    rx: &async_channel::Receiver<Event>,
    expected_count: usize,
    min_revision: u64,
) -> Status {
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        match rx.try_recv() {
            Ok(Event::IndexStatus(status)) => {
                assert!(status.error.is_none(), "{:?}", status.error);
                if !status.building
                    && status.count == expected_count
                    && status.revision >= min_revision
                {
                    return status;
                }
            }
            Ok(_) => {}
            Err(async_channel::TryRecvError::Empty) => {
                assert!(
                    Instant::now() < deadline,
                    "index maintenance wait timed out"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(async_channel::TryRecvError::Closed) => panic!("index service closed"),
        }
    }
}

#[test]
#[ignore = "Explicit real-file maintenance benchmark and profile"]
fn index_maintenance_real_files_benchmark() {
    let files = env_usize("BACKLEAF_MAINT_FILES", 20_000) / 10 * 10;
    let dirs = env_usize("BACKLEAF_MAINT_DIRS", 200).min(files.max(1));
    let identity_sample = env_usize("BACKLEAF_MAINT_IDENTITY_SAMPLE", 200);
    let changes = env_usize("BACKLEAF_MAINT_INCR_CHANGES", 500);
    let fixture = create_fixture(files.max(1), dirs.max(1));

    // 1. Enumeration only: the unavoidable directory-walk floor.
    let enumerate_start = Instant::now();
    let (enumerated, sampled_files) = enumerate_tree(&fixture.root);
    let enumerate_ms = enumerate_start.elapsed().as_secs_f64() * 1000.;

    // 2. Identity capture sample: CreateFile + FileId queries per file.
    let stride = (sampled_files.len() / identity_sample.max(1)).max(1);
    let sample: Vec<_> = sampled_files
        .iter()
        .step_by(stride)
        .take(identity_sample)
        .collect();
    let identity_start = Instant::now();
    for path in &sample {
        let _ = crate::operations::windows::identity::capture(path).unwrap();
    }
    let identity_sample_ms = identity_start.elapsed().as_secs_f64() * 1000.;
    let identity_per_file_ms = identity_sample_ms / sample.len().max(1) as f64;

    // 3. Full build from an empty state directory, with profile counters.
    index::profile::reset();
    let build_start = Instant::now();
    let db = index::build_test_index(&fixture.root, &fixture.state);
    let build_ms = build_start.elapsed().as_secs_f64() * 1000.;
    let build_enumerate_ms = index::profile::millis(&index::profile::ENUMERATE_NS);
    let build_identity_ms = index::profile::millis(&index::profile::IDENTITY_NS);
    let build_upsert_ms = index::profile::millis(&index::profile::UPSERT_NS);
    let build_unprofiled_ms = build_ms - build_enumerate_ms - build_identity_ms - build_upsert_ms;
    let build_profile = profile_json("");
    let entries: i64 = db
        .query_row("SELECT count(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    let index_bytes = file_bytes(&index::database(&fixture.state));
    let build_wal_bytes = wal_bytes(&fixture.state);
    drop(db);
    std::thread::sleep(Duration::from_millis(300));

    // 4. Startup and explicit re-scan of an unchanged tree through the real
    // writer thread (this is the current "no-change calibration" write cost).
    // The idle policy would otherwise depend on developer input.
    crate::idle::force_idle();
    let (tx, rx) = async_channel::unbounded();
    index::profile::reset();
    let startup_start = Instant::now();
    let service = Service::new(
        fixture.state.clone(),
        Some(vec![fixture.root.clone()]),
        tx.clone(),
    );
    let startup_status = wait_status(&rx, entries as usize, 0);
    let startup_ms = startup_start.elapsed().as_secs_f64() * 1000.;
    let startup_profile = profile_json("rescan_");
    let startup_wal_bytes = wal_bytes(&fixture.state);
    index::profile::reset();
    let rescan_start = Instant::now();
    service.send(Action::Rescan).unwrap();
    let rescan_status = wait_status(&rx, entries as usize, startup_status.revision + 1);
    let rescan_ms = rescan_start.elapsed().as_secs_f64() * 1000.;
    let rescan_profile = profile_json("rescan_");
    drop(service);
    drop(rx);
    drop(tx);
    std::thread::sleep(Duration::from_millis(400));
    let rescan_wal_bytes = wal_bytes(&fixture.state);

    // 5. Notification-style updates: one immediate transaction per changed file,
    // matching what ReadDirectoryChangesW currently feeds the writer.
    let incremental_root = fixture
        .root
        .canonicalize()
        .unwrap_or_else(|_| fixture.root.clone());
    let incremental_paths: Vec<PathBuf> = (0..changes)
        .map(|i| {
            let dir = incremental_root.join(format!("d{:04}", i % fixture.dirs));
            let path = dir.join(format!("incremental-{i:06}.txt"));
            std::fs::write(&path, b"incremental").unwrap();
            path
        })
        .collect();
    let mut write = index::open(&fixture.state).unwrap();
    let root_id: i64 = write
        .query_row("SELECT id FROM roots", [], |row| row.get(0))
        .unwrap();
    index::profile::reset();
    let incremental_start = Instant::now();
    for path in &incremental_paths {
        index::incremental(&mut write, root_id, path, &fixture.state).unwrap();
    }
    let incremental_ms = incremental_start.elapsed().as_secs_f64() * 1000.;
    let incremental_capture_ms = index::profile::millis(&index::profile::INCREMENTAL_CAPTURE_NS);
    let incremental_root_ms = index::profile::millis(&index::profile::INCREMENTAL_ROOT_NS);
    let incremental_upsert_ms = index::profile::millis(&index::profile::INCREMENTAL_UPSERT_NS);
    let incremental_commit_ms = index::profile::millis(&index::profile::INCREMENTAL_COMMIT_NS);
    let incremental_sql_ms = index::profile::millis(&index::profile::INCREMENTAL_SQL_NS);
    let incremental_wal_bytes = wal_bytes(&fixture.state);

    // 6. Batched notification path: 128-change transactions, matching the Service
    // drain loop plus paired old/new rename changes.
    let batched_paths: Vec<PathBuf> = (0..changes)
        .map(|i| {
            let dir = incremental_root.join(format!("d{:04}", i % fixture.dirs));
            let path = dir.join(format!("batched-{i:06}.txt"));
            std::fs::write(&path, b"batched").unwrap();
            path
        })
        .collect();
    index::profile::reset();
    let batched_wal_before = wal_bytes(&fixture.state);
    let batched_start = Instant::now();
    let mut batches = 0usize;
    for chunk in batched_paths.chunks(128) {
        let batch = chunk
            .iter()
            .map(|path| crate::index_watch::Change {
                root: root_id,
                path: path.clone(),
                from: None,
            })
            .collect::<Vec<_>>();
        index::incremental_batch(&mut write, &batch, &fixture.state).unwrap();
        batches += 1;
    }
    let batched_ms = batched_start.elapsed().as_secs_f64() * 1000.;
    let batched_capture_ms = index::profile::millis(&index::profile::INCREMENTAL_CAPTURE_NS);
    let batched_root_ms = index::profile::millis(&index::profile::INCREMENTAL_ROOT_NS);
    let batched_upsert_ms = index::profile::millis(&index::profile::INCREMENTAL_UPSERT_NS);
    let batched_commit_ms = index::profile::millis(&index::profile::INCREMENTAL_COMMIT_NS);
    let batched_sql_ms = index::profile::millis(&index::profile::INCREMENTAL_SQL_NS);
    let batched_wal_bytes = wal_bytes(&fixture.state).saturating_sub(batched_wal_before);
    drop(write);

    let report = serde_json::json!({
        "benchmark": "index-maintenance-profile",
        "generated_at_utc": chrono::Utc::now().to_rfc3339(),
        "files": fixture.files,
        "directories": fixture.dirs,
        "enumeration": {
            "ms": rounded(enumerate_ms),
            "entries": enumerated,
            "per_entry_us": rounded(enumerate_ms * 1000. / enumerated.max(1) as f64),
        },
        "identity_capture": {
            "sample_files": sample.len(),
            "ms": rounded(identity_sample_ms),
            "per_file_ms": rounded(identity_per_file_ms),
            "estimated_full_ms": rounded(identity_per_file_ms * fixture.files as f64),
        },
        "build": {
            "ms": rounded(build_ms),
            "per_file_ms": rounded(build_ms / fixture.files.max(1) as f64),
            "entries": entries,
            "index_bytes": index_bytes,
            "wal_bytes": build_wal_bytes,
            "profile": build_profile,
            "unprofiled_ms": rounded(build_unprofiled_ms),
        },
        "startup_rescan_unchanged": {
            "ms": rounded(startup_ms),
            "wal_bytes": startup_wal_bytes,
            "profile": startup_profile,
            "revision": startup_status.revision,
        },
        "explicit_rescan_unchanged": {
            "ms": rounded(rescan_ms),
            "wal_bytes": rescan_wal_bytes,
            "profile": rescan_profile,
            "revision": rescan_status.revision,
        },
        "incremental_per_change": {
            "changes": changes,
            "total_ms": rounded(incremental_ms),
            "per_change_ms": rounded(incremental_ms / changes.max(1) as f64),
            "capture_ms": rounded(incremental_capture_ms),
            "root_ms": rounded(incremental_root_ms),
            "upsert_ms": rounded(incremental_upsert_ms),
            "commit_ms": rounded(incremental_commit_ms),
            "sql_ms": rounded(incremental_sql_ms),
            "wal_bytes": incremental_wal_bytes,
        },
        "incremental_batched": {
            "changes": changes,
            "batches": batches,
            "total_ms": rounded(batched_ms),
            "per_change_ms": rounded(batched_ms / changes.max(1) as f64),
            "capture_ms": rounded(batched_capture_ms),
            "root_ms": rounded(batched_root_ms),
            "upsert_ms": rounded(batched_upsert_ms),
            "commit_ms": rounded(batched_commit_ms),
            "sql_ms": rounded(batched_sql_ms),
            "wal_bytes": batched_wal_bytes,
        },
        "scope": SCOPE,
    });
    println!("AUTO_INDEX_MAINTENANCE_BENCH {report}");
    if let Ok(path) = std::env::var("BACKLEAF_MAINT_OUT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}
