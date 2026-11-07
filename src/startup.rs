//! Opt-in cold-start tracing. Enabled by `BACKLEAF_STARTUP_REPORT=<path>`.
//! Zero work when disabled (one env lookup in main).
use std::{
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
static START: OnceLock<Instant> = OnceLock::new();
static TARGET: Mutex<Option<PathBuf>> = Mutex::new(None);
static MARKS: Mutex<Vec<(String, f64)>> = Mutex::new(Vec::new());
static READY: AtomicBool = AtomicBool::new(false);
pub fn arm() {
    let Some(path) = std::env::var_os("BACKLEAF_STARTUP_REPORT").map(PathBuf::from) else {
        return;
    };
    START.get_or_init(Instant::now);
    *TARGET.lock().unwrap() = Some(path);
    mark("main");
}
pub fn mark(stage: &str) {
    let Some(start) = START.get() else {
        return;
    };
    if TARGET.lock().unwrap().is_none() {
        return;
    }
    MARKS
        .lock()
        .unwrap()
        .push((stage.to_owned(), start.elapsed().as_secs_f64() * 1000.));
}
pub fn ready(stage: &str) {
    if READY.swap(true, Ordering::AcqRel) {
        return;
    }
    mark(stage);
    write_report();
}
fn write_report() {
    let Some(path) = TARGET.lock().unwrap().clone() else {
        return;
    };
    let marks = MARKS.lock().unwrap().clone();
    let value = serde_json::json!({"marks": marks.into_iter().map(|(stage, ms)| serde_json::json!({"stage":stage,"ms":ms})).collect::<Vec<_>>()});
    let _ = std::fs::write(path, serde_json::to_vec(&value).unwrap_or_default());
}
