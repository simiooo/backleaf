//! Explicitly invoked Shell probe. Every operated source is created in a fresh workspace fixture.
#[path = "../../../src/layout.rs"]
#[allow(dead_code)]
mod layout;
#[path = "../../../src/model.rs"]
#[allow(dead_code, unused_imports)]
mod model;
#[path = "../../../src/operations/mod.rs"]
#[allow(dead_code)]
// This probe uses the native adapter before the GPUI command layer is connected.
mod operations;
#[path = "../../../src/selection.rs"]
#[allow(dead_code)]
mod selection;
#[path = "../../../src/navigation.rs"]
#[allow(dead_code)]
mod navigation;
use operations::ledger::Ledger;
use operations::windows as shell;
use serde_json::{Value, json};
use shell::{Apartment, Collision, Kind, Notice, Observer, Step};
use std::{
    fs,
    io::Write,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
static RUN_ROOT: OnceLock<PathBuf> = OnceLock::new();
static CASE: AtomicUsize = AtomicUsize::new(0);
struct Recorder {
    events: Mutex<Vec<Value>>,
    cancel: AtomicBool,
    cancel_after: Option<u64>,
    started: Instant,
    ledger: Mutex<Ledger>,
    persist_error: Mutex<Option<String>>,
}
impl Recorder {
    fn new(cancel_after: Option<u64>, ledger: Ledger) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            cancel: AtomicBool::new(false),
            cancel_after,
            started: Instant::now(),
            ledger: Mutex::new(ledger),
            persist_error: Mutex::new(None),
        })
    }
}
impl Observer for Recorder {
    fn canceled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
    fn notice(&self, n: Notice) {
        if let Err(error) = self.ledger.lock().unwrap().record(&n) {
            *self.persist_error.lock().unwrap() = Some(error.to_string());
            self.cancel.store(true, Ordering::Release);
        }
        let mut value = match n {
            Notice::Progress {
                bytes,
                bytes_total,
                points,
                points_total,
                items,
                items_total,
            } => {
                if self
                    .cancel_after
                    .is_some_and(|limit| bytes >= limit && bytes < bytes_total)
                {
                    self.cancel.store(true, Ordering::Release);
                }
                json!({"kind":"progress","bytes":bytes,"bytes_total":bytes_total,"points":points,"points_total":points_total,"items":items,"items_total":items_total})
            }
            Notice::Before {
                key,
                source,
                flags,
                root,
            } => json!({"kind":"before","key":key,"source":source,"flags":flags,"root":root}),
            Notice::After {
                key,
                source,
                created,
                hresult,
                root,
            } => {
                json!({"kind":"after","key":key,"source":source,"created":created,"hresult":hresult,"root":root})
            }
            Notice::SinkWork { done, total } => {
                json!({"kind":"sink_work_points","done":done,"total":total})
            }
            Notice::Finished { i32_hresult } => json!({"kind":"finished","hresult":i32_hresult}),
            Notice::DialogStarted => json!({"kind":"dialog_started"}),
            Notice::DialogStopped => json!({"kind":"dialog_stopped"}),
        };
        value["at_ms"] = json!(self.started.elapsed().as_secs_f64() * 1000.);
        let mut events = self.events.lock().unwrap();
        if events.len() < 2048 {
            events.push(value);
        } else {
            *self.persist_error.lock().unwrap() = Some("bounded probe log exhausted".into());
            self.cancel.store(true, Ordering::Release);
        }
    }
}
fn step(key: u64, source: impl AsRef<Path>, dest: impl AsRef<Path>, name: Option<&str>) -> Step {
    Step {
        key,
        source: source.as_ref().to_owned(),
        destination: dest.as_ref().to_owned(),
        new_name: name.map(Into::into),
        expected: None,
        expected_destination:None,
    }
}
fn execute(
    apartment: &Apartment,
    kind: Kind,
    steps: &[Step],
    collision: Collision,
    cancel_after: Option<u64>,
) -> anyhow::Result<Value> {
    let ledger = Ledger::open(&RUN_ROOT.get().unwrap().join(format!(
        "case-{:02}.sqlite",
        CASE.fetch_add(1, Ordering::Relaxed)
    )))?;
    ledger.begin()?;
    for step in steps {
        let root = if matches!(kind, Kind::NewFolder) {
            step.destination.join(step.new_name.as_ref().unwrap())
        } else {
            step.source.clone()
        };
        let metadata_path = if matches!(kind, Kind::NewFolder) {
            &step.destination
        } else {
            &step.source
        };
        let mut metadata = shell::identity::capture(metadata_path).ok();
        if let (Some(id), Some(metadata)) = (&step.expected, metadata.as_mut()) {
            metadata.identity = id.clone();
        }
        ledger.add_root(step.key, &root, metadata.as_ref())?;
    }
    ledger.commit()?;
    ledger.begin()?;
    let observer = Recorder::new(cancel_after, ledger);
    let outcome = shell::perform(apartment, kind, steps, collision, 0, observer.clone())?;
    anyhow::ensure!(
        observer.persist_error.lock().unwrap().is_none(),
        "result persistence failed: {:?}",
        observer.persist_error.lock().unwrap()
    );
    let ledger = observer.ledger.lock().unwrap();
    ledger.finish_batch(
        &steps.iter().map(|s| s.key).collect::<Vec<_>>(),
        &outcome,
        observer.canceled(),
    )?;
    let counts = ledger.counts()?;
    let rows = ledger.page(0, 96, false)?;
    let events = observer.events.lock().unwrap().clone();
    Ok(
        json!({"outcome":{"perform_hresult":outcome.perform_hresult,"aborted":outcome.aborted,"queued":outcome.queued,"queue_errors":outcome.queue_errors},"counts":{"discovered":counts.discovered,"successful":counts.successful,"failed":counts.failed,"skipped":counts.skipped,"canceled":counts.canceled,"partial":counts.partial,"unknown":counts.unknown,"not_processed":counts.not_processed},"results":rows.iter().map(|r|json!({"sequence":r.sequence,"root":r.root,"path":r.path,"state":format!("{:?}",r.state),"hresult":r.hresult,"created":r.created})).collect::<Vec<_>>(),"events":events,"duration_ms":observer.started.elapsed().as_secs_f64()*1000.}),
    )
}
fn main() -> anyhow::Result<()> {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let fixtures = project.join(".local/operations-probe");
    fs::create_dir_all(&fixtures)?;
    let run = fixtures.join(format!(
        "run-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&run)?;
    let run = run.canonicalize()?;
    anyhow::ensure!(
        run.starts_with(fixtures.canonicalize()?),
        "fixture escaped workspace"
    );
    RUN_ROOT.set(run.clone()).unwrap();
    let source = run.join("source");
    let destination = run.join("destination");
    fs::create_dir(&source)?;
    fs::create_dir(&destination)?;
    if std::env::args().any(|arg| arg == "--long-path-only") {
        let deep = source
            .join("长路径资料".repeat(16))
            .join("研究记录".repeat(20))
            .join("最终材料".repeat(20));
        fs::create_dir_all(&deep)?;
        fs::write(deep.join("长文件名.txt"), b"long-path")?;
        let target = destination
            .join("长路径目标".repeat(16))
            .join("研究记录".repeat(20))
            .join("最终材料".repeat(20));
        fs::create_dir_all(&target)?;
        let apartment = Apartment::new()?;
        let mut result = execute(
            &apartment,
            Kind::Copy,
            &[step(1, deep.join("长文件名.txt"), &target, None)],
            Collision::Ask,
            None,
        )?;
        result["source"] = json!(deep.join("长文件名.txt"));
        result["destination"] = json!(target.join("长文件名.txt"));
        result["content_equal"] =
            json!(fs::read(target.join("长文件名.txt")).ok().as_deref() == Some(b"long-path"));
        fs::write(
            run.join("long-path.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        println!(
            "{}",
            json!({"report":run.join("long-path.json"),"outcome":result["outcome"],"content_equal":result["content_equal"]})
        );
        return Ok(());
    }
    let filename = "中文_👩🏽‍💻_é.bin";
    let mut file = fs::File::create(source.join(filename))?;
    let block = vec![0x5a; 1024 * 1024];
    for _ in 0..64 {
        file.write_all(&block)?;
    }
    drop(file);
    let tree = source.join("tree");
    fs::create_dir(&tree)?;
    for i in 0..16 {
        fs::write(
            tree.join(format!("文件_{i:03}.txt")),
            format!("content-{i}"),
        )?;
    }
    let apartment = Apartment::new()?;
    let mut cases = Vec::new();
    let mut copied = execute(
        &apartment,
        Kind::Copy,
        &[
            step(1, source.join(filename), &destination, None),
            step(2, &tree, &destination, None),
        ],
        Collision::Ask,
        None,
    )?;
    copied["name"] = json!("file_and_recursive_copy");
    copied["content_equal"] = json!(
        fs::read(source.join(filename))? == fs::read(destination.join(filename))?
            && fs::read_dir(&tree)?.all(|e| {
                let e = e.unwrap();
                fs::read(e.path()).ok()
                    == fs::read(destination.join("tree").join(e.file_name())).ok()
            })
    );
    cases.push(copied);
    let canceled_dest = run.join("cancel-destination");
    fs::create_dir(&canceled_dest)?;
    let mut canceled = execute(
        &apartment,
        Kind::Copy,
        &[
            step(3, source.join(filename), &canceled_dest, None),
            step(4, &tree, &canceled_dest, None),
        ],
        Collision::Ask,
        Some(8 * 1024 * 1024),
    )?;
    canceled["name"] = json!("in_flight_cancel");
    canceled["remaining_root_not_started"] = json!(
        !canceled["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "before" && e["key"] == 4)
    );
    canceled["partial_target_bytes"] = json!(
        fs::metadata(canceled_dest.join(filename))
            .map(|m| m.len())
            .ok()
    );
    cases.push(canceled);
    fs::write(source.join("original.txt"), b"rename-and-move")?;
    let mut renamed = execute(
        &apartment,
        Kind::Rename,
        &[step(
            5,
            source.join("original.txt"),
            Path::new(""),
            Some("名称_é_👩🏽‍💻.txt"),
        )],
        Collision::Ask,
        None,
    )?;
    renamed["name"] = json!("rename_unicode");
    renamed["verified"] = json!(
        !source.join("original.txt").exists()
            && fs::read(source.join("名称_é_👩🏽‍💻.txt"))? == b"rename-and-move"
    );
    cases.push(renamed);
    let mut moved = execute(
        &apartment,
        Kind::Move,
        &[step(6, source.join("名称_é_👩🏽‍💻.txt"), &destination, None)],
        Collision::Ask,
        None,
    )?;
    moved["name"] = json!("same_volume_move");
    moved["verified"] = json!(
        !source.join("名称_é_👩🏽‍💻.txt").exists()
            && fs::read(destination.join("名称_é_👩🏽‍💻.txt"))? == b"rename-and-move"
    );
    cases.push(moved);
    let mut created = execute(
        &apartment,
        Kind::NewFolder,
        &[step(7, Path::new(""), &destination, Some("新建文件夹"))],
        Collision::Ask,
        None,
    )?;
    created["name"] = json!("new_folder");
    created["verified"] = json!(destination.join("新建文件夹").is_dir());
    cases.push(created);
    fs::write(source.join("conflict.txt"), b"new-content")?;
    fs::write(destination.join("conflict.txt"), b"existing-content")?;
    let mut kept = execute(
        &apartment,
        Kind::Copy,
        &[step(8, source.join("conflict.txt"), &destination, None)],
        Collision::KeepBoth,
        None,
    )?;
    kept["name"] = json!("keep_both");
    kept["original_preserved"] =
        json!(fs::read(destination.join("conflict.txt"))? == b"existing-content");
    cases.push(kept);
    let mut replaced = execute(
        &apartment,
        Kind::Copy,
        &[step(9, source.join("conflict.txt"), &destination, None)],
        Collision::Replace,
        None,
    )?;
    replaced["name"] = json!("explicit_replace");
    replaced["verified"] = json!(fs::read(destination.join("conflict.txt"))? == b"new-content");
    cases.push(replaced);
    let mut missing = execute(
        &apartment,
        Kind::Copy,
        &[step(
            10,
            source.join("does-not-exist.txt"),
            &destination,
            None,
        )],
        Collision::Ask,
        None,
    )?;
    missing["name"] = json!("missing_source");
    missing["no_target_created"] = json!(!destination.join("does-not-exist.txt").exists());
    cases.push(missing);
    let failed_tree = source.join("partly-locked");
    fs::create_dir(&failed_tree)?;
    fs::write(failed_tree.join("locked.txt"), b"locked")?;
    fs::write(failed_tree.join("readable.txt"), b"readable")?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(failed_tree.join("locked.txt"))?;
    let mut locked = execute(
        &apartment,
        Kind::Copy,
        &[step(11, &failed_tree, &destination, None)],
        Collision::Ask,
        None,
    )?;
    drop(lock);
    locked["name"] = json!("recursive_child_failure");
    locked["readable_copied"] = json!(destination.join("partly-locked/readable.txt").is_file());
    locked["locked_not_copied"] = json!(!destination.join("partly-locked/locked.txt").exists());
    cases.push(locked);
    let deep = source
        .join("长路径资料".repeat(10))
        .join("研究记录".repeat(12))
        .join("最终材料".repeat(12));
    fs::create_dir_all(&deep)?;
    fs::write(deep.join("长文件名.txt"), b"long-path")?;
    let mut long = execute(
        &apartment,
        Kind::Copy,
        &[step(12, deep.join("长文件名.txt"), &destination, None)],
        Collision::Ask,
        None,
    )?;
    long["name"] = json!("long_path");
    long["verified"] =
        json!(fs::read(destination.join("长文件名.txt")).ok().as_deref() == Some(b"long-path"));
    cases.push(long);
    // One tiny, unique fixture exercises real recycling; no user paths are selected.
    let trash = source.join(format!("Backleaf-recycle-probe-{}.txt", std::process::id()));
    fs::write(&trash, b"Backleaf isolated recycle validation")?;
    let mut recycled = execute(
        &apartment,
        Kind::Recycle,
        &[step(13, &trash, Path::new(""), None)],
        Collision::Ask,
        None,
    )?;
    recycled["name"] = json!("recycle");
    recycled["source_removed"] = json!(!trash.exists());
    cases.push(recycled);
    let changing = source.join("identity-changing.txt");
    fs::write(&changing, b"original-object")?;
    let metadata = shell::identity::capture(&changing)?;
    anyhow::ensure!(!metadata.directory && !metadata.reparse && metadata.bytes == Some(15));
    let expected = metadata.identity;
    anyhow::ensure!(
        shell::identity::Identity::decode(&expected.encode()).as_ref() == Some(&expected)
    );
    fs::rename(&changing, source.join("identity-original.txt"))?;
    fs::write(&changing, b"replacement-object")?;
    let mut changed_step = step(14, &changing, Path::new(""), None);
    changed_step.expected = Some(expected);
    let mut changed = execute(
        &apartment,
        Kind::Recycle,
        &[changed_step],
        Collision::Ask,
        None,
    )?;
    changed["name"] = json!("source_identity_changed");
    changed["replacement_preserved"] = json!(fs::read(&changing)? == b"replacement-object");
    changed["original_preserved"] =
        json!(fs::read(source.join("identity-original.txt"))? == b"original-object");
    cases.push(changed);
    let report = json!({"kind":"headless Shell interface probe; not a GPUI UI acceptance","fixture":run,"source_file_bytes":67108864u64,"cases":cases});
    fs::write(run.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{}",
        json!({"report":run.join("report.json"),"cases":report["cases"].as_array().unwrap().iter().map(|c|json!({"name":c["name"],"outcome":c["outcome"]})).collect::<Vec<_>>()})
    );
    Ok(())
}
