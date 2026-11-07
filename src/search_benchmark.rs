//! Ignored real-file benchmark: default search through the persistent index vs the
//! live filesystem scan it replaced. Run with:
//! `cargo test --release --locked -p backleaf search_benchmark:: -- --ignored --nocapture`
//! Optional environment: BACKLEAF_BENCH_FILES, BACKLEAF_BENCH_DIRS,
//! BACKLEAF_BENCH_COLD_RUNS, BACKLEAF_BENCH_WARM_RUNS, BACKLEAF_BENCH_OUT.
use crate::file_view::FileView;
use crate::model::Event;
use crate::search::{Notice, Worker};
use async_channel::TryRecvError;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SCOPE: &str = "Real owned files on one NTFS directory tree, production search worker and bounded first page; OS file cache warm after fixture creation; worker-level latency only, no UI/GPU/input-to-photon or cold-disk claim.";

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
    state_index: PathBuf,
    state_live: PathBuf,
    files: usize,
    dirs: usize,
    cjk_matches: usize,
    note_matches: usize,
    note_prefix_matches: usize,
    note_wide_matches: usize,
    report_prefix_matches: usize,
    local_matches: usize,
    index_build_ms: f64,
    index_bytes: u64,
    index: rusqlite::Connection,
}

fn create_fixture(files: usize, dirs: usize) -> Fixture {
    let fixture = crate::test_support::Fixture::new("search-benchmark");
    let root = fixture.0.join("files");
    std::fs::create_dir(&root).unwrap();
    let mut dir_paths = Vec::with_capacity(dirs);
    for dir in 0..dirs {
        let path = root.join(format!("d{dir:04}"));
        std::fs::create_dir(&path).unwrap();
        dir_paths.push(path);
    }
    let (mut cjk_matches, mut note_matches, mut note_prefix_matches) = (0, 0, 0);
    let (mut note_wide_matches, mut report_prefix_matches) = (0, 0);
    for i in 0..files {
        let name = match i % 10 {
            0..=4 => {
                cjk_matches += 1;
                if i < 1000 {
                    report_prefix_matches += 1;
                }
                format!("报告_{i:06}.txt")
            }
            5..=7 => format!("数据_{i:06}.csv"),
            _ => {
                note_matches += 1;
                if i < 1000 {
                    note_prefix_matches += 1;
                }
                if i < 10000 {
                    note_wide_matches += 1;
                }
                format!("note-{i:06}.md")
            }
        };
        std::fs::write(dir_paths[i % dirs].join(name), b"benchmark").unwrap();
    }
    std::fs::write(dir_paths[0].join("needle-unique-report.txt"), b"benchmark").unwrap();
    let local_matches = 60usize;
    for i in 0..local_matches {
        std::fs::write(dir_paths[0].join(format!("local-{i:03}.txt")), b"benchmark").unwrap();
    }
    let state_index = fixture.0.join("state-index");
    let state_live = fixture.0.join("state-live");
    let start = Instant::now();
    let index = crate::index::build_test_index(&root, &state_index);
    let index_build_ms = start.elapsed().as_secs_f64() * 1000.;
    let index_bytes = std::fs::metadata(crate::index::database(&state_index))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    // A valid but empty index is what an unindexed location sees in production:
    // the planner opens it, finds no covering root and keeps the live scan.
    drop(crate::index::open(&state_live).unwrap());
    Fixture {
        _fixture: fixture,
        root,
        state_index,
        state_live,
        files,
        dirs,
        cjk_matches,
        note_matches,
        note_prefix_matches,
        note_wide_matches,
        report_prefix_matches,
        local_matches,
        index_build_ms,
        index_bytes,
        index,
    }
}

fn run_query(
    worker: &Worker,
    rx: &async_channel::Receiver<Event>,
    view: &FileView,
    token: u64,
) -> (f64, f64, usize) {
    let start = Instant::now();
    worker.search_view(token, view.clone());
    worker.page(token, 1, 0..32);
    let mut first_page = None;
    let deadline = start + Duration::from_secs(180);
    loop {
        match rx.try_recv() {
            Ok(Event::Search(Notice::Page { token: event, .. })) if event == token => {
                if first_page.is_none() {
                    first_page = Some(start.elapsed().as_secs_f64() * 1000.);
                }
            }
            Ok(Event::Search(Notice::Progress {
                token: event,
                count,
                done: true,
                error,
                ..
            })) if event == token => {
                assert!(error.is_none(), "{error:?}");
                let done = start.elapsed().as_secs_f64() * 1000.;
                return (first_page.unwrap_or(done), done, count);
            }
            Ok(_) => {}
            Err(TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "search benchmark timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(TryRecvError::Closed) => panic!("search worker disconnected"),
        }
    }
}

fn warmup_view(view: &FileView) -> FileView {
    let mut warmup = view.clone();
    warmup.query = "zzzz-no-such-name".into();
    warmup
}

#[derive(Default)]
struct Timings {
    first_page_ms: Vec<f64>,
    done_ms: Vec<f64>,
    count: usize,
    indexed: bool,
}

fn measure(state: &Path, view: &FileView, cold_runs: usize, warm_runs: usize) -> Timings {
    let mut timings = Timings::default();
    for run in 0..cold_runs {
        let (tx, rx) = async_channel::unbounded();
        let worker = Worker::with_state_dir(tx, state.to_path_buf());
        // Let the worker initialize and leave one unrelated cache entry, matching a
        // long-lived production worker. The measured query is still a cache miss.
        let _ = run_query(&worker, &rx, &warmup_view(view), 1);
        let (first_page, done, count) = run_query(&worker, &rx, view, 2);
        println!(
            "BENCH_PHASE cold run {run} query={} first_page={first_page:.1} done={done:.1} count={count}",
            view.query
        );
        timings.first_page_ms.push(first_page);
        timings.done_ms.push(done);
        timings.count = count;
        timings.indexed = worker.used_index();
        drop(worker);
        drop(rx);
    }
    println!("BENCH_PHASE warm query={}", view.query);
    let (tx, rx) = async_channel::unbounded();
    let worker = Worker::with_state_dir(tx, state.to_path_buf());
    let _ = run_query(&worker, &rx, &warmup_view(view), 1);
    let _ = run_query(&worker, &rx, view, 2);
    for run in 0..warm_runs {
        let (first_page, done, _) = run_query(&worker, &rx, view, 3 + run as u64);
        println!("BENCH_PHASE warm run {run} first_page={first_page:.1} done={done:.1}");
        timings.first_page_ms.push(first_page);
        timings.done_ms.push(done);
    }
    timings.indexed = worker.used_index();
    drop(worker);
    drop(rx);
    timings
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    if sorted.is_empty() {
        return 0.;
    }
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.
    } else {
        sorted[middle]
    }
}

fn rounded(value: f64) -> f64 {
    (value * 1000.).round() / 1000.
}

#[test]
#[ignore = "Explicit real-file benchmark for automatic index search"]
fn automatic_index_search_real_files_benchmark() {
    let files = (env_usize("BACKLEAF_BENCH_FILES", 20_000) / 10 * 10).max(10);
    let dirs = env_usize("BACKLEAF_BENCH_DIRS", 100).min(files);
    let cold_runs = env_usize("BACKLEAF_BENCH_COLD_RUNS", 3);
    let warm_runs = env_usize("BACKLEAF_BENCH_WARM_RUNS", 2);
    let fixture = create_fixture(files, dirs);
    println!(
        "BENCH_PHASE fixture files={files} dirs={dirs} index_build_ms={:.1} index_bytes={}",
        fixture.index_build_ms, fixture.index_bytes
    );
    let local_root = fixture.root.join("d0000").canonicalize().unwrap();
    let expected = [
        ("unique", "needle-unique", 1usize, None),
        ("medium", "note-000", fixture.note_prefix_matches, None),
        ("cjk_1_char", "报", fixture.cjk_matches, None),
        ("ascii_broad", "note", fixture.note_matches, None),
        (
            "folder_local",
            "local",
            fixture.local_matches,
            Some(local_root.clone()),
        ),
        ("ascii_2_short", "ue", 1usize, None),
        ("ascii_1_short", "q", 1usize, None),
        (
            "folder_local_short",
            "lo",
            fixture.local_matches,
            Some(local_root.clone()),
        ),
    ];
    let mut scenarios = Vec::new();
    for (name, query, expected_count, scenario_root) in expected {
        let mut view = FileView::new(scenario_root.as_ref().unwrap_or(&fixture.root));
        view.query = query.into();
        println!("BENCH_PHASE scenario {name} auto");
        let auto = measure(&fixture.state_index, &view, cold_runs, warm_runs);
        println!("BENCH_PHASE scenario {name} live");
        let live = measure(&fixture.state_live, &view, cold_runs, warm_runs);
        assert!(
            !live.indexed,
            "{name}: unindexed location did not fall back"
        );
        assert_eq!(
            (auto.count, live.count, expected_count),
            (expected_count, expected_count, expected_count),
            "{name}: automatic and live scan counts diverged"
        );
        let auto_done = median(&auto.done_ms);
        let live_done = median(&live.done_ms);
        let auto_first = median(&auto.first_page_ms);
        let live_first = median(&live.first_page_ms);
        println!(
            "AUTO_INDEX_BENCH_SCENARIO {}",
            serde_json::json!({
                "scenario": name,
                "query": query,
                "matches": auto.count,
                "auto_used_index": auto.indexed,
                "auto_cold_page_ms": auto.first_page_ms.iter().take(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "live_cold_page_ms": live.first_page_ms.iter().take(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "auto_cold_done_ms": auto.done_ms.iter().take(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "live_cold_done_ms": live.done_ms.iter().take(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "auto_warm_done_ms": auto.done_ms.iter().skip(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "live_warm_done_ms": live.done_ms.iter().skip(cold_runs).map(|value| rounded(*value)).collect::<Vec<_>>(),
                "auto_done_median_ms": rounded(auto_done),
                "live_done_median_ms": rounded(live_done),
                "done_speedup": rounded(live_done / auto_done),
                "auto_page_median_ms": rounded(auto_first),
                "live_page_median_ms": rounded(live_first),
                "page_speedup": rounded(live_first / auto_first),
            })
        );
        scenarios.push(serde_json::json!({
            "scenario": name,
            "query": query,
            "matches": auto.count,
            "auto_used_index": auto.indexed,
            "cold_runs": cold_runs,
            "warm_runs": warm_runs,
            "auto": {
                "first_page_ms": auto.first_page_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
                "done_ms": auto.done_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            },
            "live": {
                "first_page_ms": live.first_page_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
                "done_ms": live.done_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            },
            "auto_done_median_ms": rounded(auto_done),
            "live_done_median_ms": rounded(live_done),
            "done_speedup": rounded(live_done / auto_done),
            "auto_page_median_ms": rounded(auto_first),
            "live_page_median_ms": rounded(live_first),
            "page_speedup": rounded(live_first / auto_first),
        }));
    }
    let mut local_view = FileView::new(&local_root);
    local_view.query = "local".into();
    crate::search::set_bulk_validate(false);
    println!("BENCH_PHASE bulk_validation off");
    let no_bulk = measure(&fixture.state_index, &local_view, cold_runs, warm_runs);
    crate::search::set_bulk_validate(true);
    println!("BENCH_PHASE bulk_validation on");
    let bulk = measure(&fixture.state_index, &local_view, cold_runs, warm_runs);
    assert_eq!(
        (no_bulk.count, bulk.count),
        (fixture.local_matches, fixture.local_matches),
        "bulk validation changed the result count"
    );
    assert!(
        no_bulk.indexed && bulk.indexed,
        "folder-local scenario did not use the index"
    );
    let no_bulk_page = median(&no_bulk.first_page_ms);
    let bulk_page = median(&bulk.first_page_ms);
    let no_bulk_done = median(&no_bulk.done_ms);
    let bulk_done = median(&bulk.done_ms);
    let bulk_validation_ab = serde_json::json!({
        "scenario": "folder_local",
        "matches": bulk.count,
        "cold_runs": cold_runs,
        "warm_runs": warm_runs,
        "no_bulk": {
            "first_page_ms": no_bulk.first_page_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            "done_ms": no_bulk.done_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            "first_page_median_ms": rounded(no_bulk_page),
            "done_median_ms": rounded(no_bulk_done),
        },
        "bulk": {
            "first_page_ms": bulk.first_page_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            "done_ms": bulk.done_ms.iter().map(|value| rounded(*value)).collect::<Vec<_>>(),
            "first_page_median_ms": rounded(bulk_page),
            "done_median_ms": rounded(bulk_done),
        },
        "page_speedup": rounded(no_bulk_page / bulk_page),
        "done_speedup": rounded(no_bulk_done / bulk_done),
    });
    println!("AUTO_INDEX_BENCH_BULK_VALIDATION {bulk_validation_ab}");
    // Extended 2-character grams vs the instr fallback: the build/index cost must
    // buy a measurable query win to stay enabled.
    let mut short_view = FileView::new(&fixture.root);
    short_view.query = "ue".into();
    let grams_on = measure(&fixture.state_index, &short_view, cold_runs, warm_runs);
    fixture
        .index
        .execute("DELETE FROM meta WHERE key='grams_v2'", [])
        .unwrap();
    let grams_off = measure(&fixture.state_index, &short_view, cold_runs, warm_runs);
    fixture
        .index
        .execute("INSERT OR IGNORE INTO meta VALUES('grams_v2',1)", [])
        .unwrap();
    assert_eq!(
        grams_on.count, grams_off.count,
        "short-query gram A/B counts diverged"
    );
    assert!(grams_on.indexed && grams_off.indexed);
    let short_query_gram_ab = serde_json::json!({
        "scenario": "ascii_2_short",
        "query": "ue",
        "matches": grams_on.count,
        "grams": {
            "first_page_median_ms": rounded(median(&grams_on.first_page_ms)),
            "done_median_ms": rounded(median(&grams_on.done_ms)),
        },
        "instr": {
            "first_page_median_ms": rounded(median(&grams_off.first_page_ms)),
            "done_median_ms": rounded(median(&grams_off.done_ms)),
        },
        "page_speedup": rounded(median(&grams_off.first_page_ms) / median(&grams_on.first_page_ms)),
        "done_speedup": rounded(median(&grams_off.done_ms) / median(&grams_on.done_ms)),
    });
    println!("AUTO_INDEX_BENCH_SHORT_GRAM {short_query_gram_ab}");
    let report = serde_json::json!({
        "benchmark": "default-search-index-vs-live-scan",
        "generated_at_utc": chrono::Utc::now().to_rfc3339(),
        "files": fixture.files,
        "directories": fixture.dirs,
        "index_build_ms": rounded(fixture.index_build_ms),
        "index_db_bytes": fixture.index_bytes,
        "planner": "index::plan_scope + index::scope_size/estimate_matches selectivity guard + scoped query",
        "matching_counts": {
            "cjk_1_char": fixture.cjk_matches,
            "note": fixture.note_matches,
            "note_prefix": fixture.note_prefix_matches,
            "folder_local": fixture.local_matches,
        },
        "cold_runs": cold_runs,
        "warm_runs": warm_runs,
        "scenarios": scenarios,
        "bulk_validation_ab": bulk_validation_ab,
        "short_query_gram_ab": short_query_gram_ab,
        "scope": SCOPE,
    });
    println!("AUTO_INDEX_SEARCH_BENCH {}", report);
    if let Ok(path) = std::env::var("BACKLEAF_BENCH_OUT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    drop(fixture.index);
}

#[test]
#[ignore = "Explicit selectivity-guard threshold sweep"]
fn match_budget_threshold_sweep_real_files() {
    use std::sync::atomic::Ordering;
    let files = (env_usize("BACKLEAF_BENCH_FILES", 20_000) / 10 * 10).max(10);
    let dirs = env_usize("BACKLEAF_BENCH_DIRS", 200).min(files);
    let cold_runs = env_usize("BACKLEAF_BENCH_COLD_RUNS", 2);
    let warm_runs = env_usize("BACKLEAF_BENCH_WARM_RUNS", 1);
    let fixture = create_fixture(files, dirs);
    let root = fixture.root.clone();
    let scope = crate::index::plan_scope(&fixture.index, &root, true)
        .unwrap()
        .expect("fixture root must be planned");
    let scope_size = crate::index::scope_size(&fixture.index, &scope).unwrap();
    let scenarios = [
        ("unique", "needle-unique", 1usize),
        ("small_note_prefix", "note-000", fixture.note_prefix_matches),
        (
            "mid_report_prefix",
            "报告_000",
            fixture.report_prefix_matches,
        ),
        ("wide_note_prefix", "note-00", fixture.note_wide_matches),
        ("broad_note", "note", fixture.note_matches),
        ("cjk_1_char", "报", fixture.cjk_matches),
    ];
    let mut live_done = Vec::new();
    for (name, query, expected) in scenarios {
        let mut view = FileView::new(&root);
        view.query = query.into();
        let live = measure(&fixture.state_live, &view, cold_runs, warm_runs);
        assert!(!live.indexed, "{name}: live baseline used the index");
        assert_eq!(live.count, expected, "{name}: live count");
        live_done.push(median(&live.done_ms));
    }
    let mut rows = Vec::new();
    for divisor in [8usize, 24, 64] {
        crate::search::MATCH_BUDGET_DIVISOR.store(divisor, Ordering::Relaxed);
        let cap = (scope_size / divisor).clamp(96, 4096);
        for (index, (name, query, expected)) in scenarios.iter().enumerate() {
            let mut view = FileView::new(&root);
            view.query = (*query).into();
            let auto = measure(&fixture.state_index, &view, cold_runs, warm_runs);
            assert_eq!(auto.count, *expected, "{name}: auto count");
            let auto_done = median(&auto.done_ms);
            let live = live_done[index];
            let row = serde_json::json!({
                "divisor": divisor,
                "cap": cap,
                "scenario": name,
                "matches": auto.count,
                "auto_used_index": auto.indexed,
                "auto_done_median_ms": rounded(auto_done),
                "live_done_median_ms": rounded(live),
                "speedup": rounded(live / auto_done),
            });
            println!("MATCH_BUDGET_SWEEP {row}");
            rows.push(row);
        }
    }
    crate::search::MATCH_BUDGET_DIVISOR.store(0, Ordering::Relaxed);

    let report = serde_json::json!({
        "benchmark": "selectivity-guard-threshold-sweep",
        "generated_at_utc": chrono::Utc::now().to_rfc3339(),
        "files": fixture.files,
        "directories": fixture.dirs,
        "scope_size": scope_size,
        "divisors": [8, 24, 64],
        "clamp": [96, 4096],
        "cold_runs": cold_runs,
        "warm_runs": warm_runs,
        "rows": rows,
        "scope": SCOPE,
    });
    println!("AUTO_INDEX_MATCH_BUDGET_SWEEP {report}");
    if let Ok(path) = std::env::var("BACKLEAF_BENCH_SWEEP_OUT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    drop(fixture.index);
}
