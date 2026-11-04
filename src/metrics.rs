use serde::Serialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Options {
    pub fixture: Option<usize>,
    pub path: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub seconds: u64,
    pub scroll: bool,
    pub flow: usize,
    pub reduced_motion: bool,
    pub width: f32,
    pub height: f32,
    pub theme: Option<bool>,
    pub selection_smoke: bool,
    pub state_dir: PathBuf,
}
impl Options {
    pub fn parse() -> Self {
        let mut result = Self {
            fixture: None,
            path: None,
            output: None,
            seconds: 12,
            scroll: false,
            flow: 1,
            reduced_motion: false,
            width: 1120.,
            height: 720.,
            theme: None,
            selection_smoke: false,
            state_dir: std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("Backleaf/operations"),
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fixture" => {
                    result.fixture =
                        Some(args.next().expect("fixture count").parse().expect("count"))
                }
                "--path" => result.path = Some(args.next().expect("path").into()),
                "--state-dir" => result.state_dir = args.next().expect("state directory").into(),
                "--bench-output" => result.output = Some(args.next().expect("output").into()),
                "--seconds" => result.seconds = args.next().unwrap().parse().unwrap(),
                "--scroll" => result.scroll = true,
                "--flow" => result.flow = args.next().unwrap().parse::<usize>().unwrap().min(8),
                "--reduced-motion" => result.reduced_motion = true,
                "--width" => result.width = args.next().unwrap().parse().unwrap(),
                "--height" => result.height = args.next().unwrap().parse().unwrap(),
                "--theme" => {
                    result.theme = Some(match args.next().as_deref() {
                        Some("light") => false,
                        Some("dark") => true,
                        _ => panic!("theme must be light or dark"),
                    })
                }
                "--selection-smoke" => result.selection_smoke = true,
                _ => panic!("unknown argument {arg}"),
            }
        }
        if result.selection_smoke {
            assert!(
                result.fixture.is_some_and(|count| count >= 100_000) && result.output.is_some(),
                "selection smoke requires a large fixture and report path"
            );
            result.flow = 0;
            result.scroll = false;
        }
        result
    }
}
#[derive(Default, Serialize)]
pub struct Distribution {
    pub samples: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}
fn distribution(values: &[f64]) -> Distribution {
    if values.is_empty() {
        return Distribution::default();
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    Distribution {
        samples: v.len(),
        p50_ms: v[(v.len() - 1) / 2],
        p95_ms: v[(v.len() as f64 * 0.95).ceil() as usize - 1],
        p99_ms: v[(v.len() as f64 * 0.99).ceil() as usize - 1],
        max_ms: *v.last().unwrap(),
    }
}
#[derive(Serialize)]
pub struct Report {
    pub session_initially_locked: Option<bool>,
    pub session_locked_at_end: bool,
    pub accessibility_active: bool,
    pub version: &'static str,
    pub gpui_revision: &'static str,
    pub backend: &'static str,
    pub dataset: String,
    pub item_count: usize,
    pub selected_count: usize,
    pub selection_bitmap_serialized_bytes: usize,
    pub selection_pending: bool,
    pub theme_dark: bool,
    pub scripted_scroll: bool,
    pub synthetic_selection_script: bool,
    pub flow_limit: usize,
    pub reduced_motion: bool,
    pub logical_viewport: [f32; 2],
    pub scale_factor: f32,
    pub duration_seconds: f64,
    pub scene_cpu: Distribution,
    pub frame_callback_interval: Distribution,
    pub input_handler_to_scene: Distribution,
    pub frames: usize,
    pub grid_builds: u64,
    pub label_layout_builds: u64,
    pub label_cache_peak: usize,
    pub visible_cells_peak: usize,
    pub cached_items_peak: usize,
    pub flow_layout_builds: u64,
    pub animation_requests: u64,
    pub max_pending_frame_callbacks: u32,
    pub hidden_animation_requests: u64,
    pub peak_working_set_mib: f64,
    pub private_bytes_mib: f64,
    pub cpu_seconds: f64,
    pub cpu_one_core_percent: f64,
    pub events: Vec<String>,
    pub measurement_notes: &'static str,
}
pub struct Metrics {
    pub session_initially_locked: Option<bool>,
    pub session_locked: bool,
    pub accessibility_active: bool,
    pub started: Instant,
    pub last_frame: Option<Instant>,
    pub render_start: Instant,
    pub input: Option<Instant>,
    pub cpu: Vec<f64>,
    pub gaps: Vec<f64>,
    pub inputs: Vec<f64>,
    pub cells: usize,
    pub cache: usize,
    pub requests: u64,
    pub grid_builds: u64,
    pub active_clocks: u32,
    pub max_clocks: u32,
    pub hidden_requests: u64,
    pub events: Vec<String>,
    pub viewport: [f32; 2],
    pub scale: f32,
    pending: bool,
}
impl Metrics {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            session_initially_locked: None,
            session_locked: true,
            accessibility_active: false,
            started: now,
            last_frame: None,
            render_start: now,
            input: None,
            cpu: vec![],
            gaps: vec![],
            inputs: vec![],
            cells: 0,
            cache: 0,
            requests: 0,
            grid_builds: 0,
            active_clocks: 0,
            max_clocks: 0,
            hidden_requests: 0,
            events: vec![],
            viewport: [0., 0.],
            scale: 1.,
            pending: false,
        }
    }
    pub fn input(&mut self, kind: &str) {
        self.input = Some(Instant::now());
        self.event(kind);
    }
    pub fn event(&mut self, kind: &str) {
        if self.events.len() < 512 {
            self.events.push(format!(
                "{:.3}s {kind}",
                self.started.elapsed().as_secs_f64()
            ));
        }
    }
    pub fn begin(&mut self, viewport: [f32; 2], scale: f32) {
        if !self.pending {
            self.render_start = Instant::now();
            self.pending = true;
        }
        self.viewport = viewport;
        self.scale = scale;
    }
    pub fn painted(&mut self) {
        if !self.pending {
            return;
        }
        self.pending = false;
        let now = Instant::now();
        // Warmup excludes first second's shader/font/device initialization; wall time and memory include it.
        if self.started.elapsed() > Duration::from_secs(1) {
            if self.cpu.len() < 100_000 {
                self.cpu
                    .push((now - self.render_start).as_secs_f64() * 1000.);
            }
            if let Some(previous) = self.last_frame
                && self.gaps.len() < 100_000
            {
                self.gaps.push((now - previous).as_secs_f64() * 1000.);
            }
            if let Some(input) = self.input.take() {
                self.inputs.push((now - input).as_secs_f64() * 1000.);
            }
        }
        self.last_frame = Some(now);
    }
    pub fn report(
        &self,
        options: &Options,
        count: usize,
        builds: u64,
        labels: (u64, usize),
    ) -> Report {
        let (peak, private, cpu_seconds) = native_process_metrics();
        let duration = self.started.elapsed().as_secs_f64();
        Report {
            session_initially_locked: self.session_initially_locked,
            session_locked_at_end: self.session_locked,
            accessibility_active: self.accessibility_active,
            version: env!("CARGO_PKG_VERSION"),
            gpui_revision: "d36a70661471ec64dce230798121d554930d5c60",
            backend: "GPUI / Win32 / Direct3D 11 / DirectWrite",
            dataset: if options.fixture.is_some() {
                "generated metadata fixture (same native grid)".into()
            } else {
                "real directory snapshot / SQLite worker".into()
            },
            item_count: count,
            selected_count: 0,
            selection_bitmap_serialized_bytes: 0,
            selection_pending: false,
            theme_dark: false,
            scripted_scroll: options.scroll,
            synthetic_selection_script: options.selection_smoke,
            flow_limit: options.flow,
            reduced_motion: options.reduced_motion,
            logical_viewport: self.viewport,
            scale_factor: self.scale,
            duration_seconds: duration,
            scene_cpu: distribution(&self.cpu),
            frame_callback_interval: distribution(&self.gaps),
            input_handler_to_scene: distribution(&self.inputs),
            frames: self.cpu.len(),
            grid_builds: self.grid_builds,
            label_layout_builds: labels.0,
            label_cache_peak: labels.1,
            visible_cells_peak: self.cells,
            cached_items_peak: self.cache,
            flow_layout_builds: builds,
            animation_requests: self.requests,
            max_pending_frame_callbacks: self.max_clocks,
            hidden_animation_requests: self.hidden_requests,
            peak_working_set_mib: peak,
            private_bytes_mib: private,
            cpu_seconds,
            cpu_one_core_percent: cpu_seconds / duration * 100.,
            events: self.events.clone(),
            measurement_notes: "Native release executable, real GPUI layout/paint and D3D11 window. Scene CPU is render start through final canvas paint; callback intervals are not GPU fence/present latency. Input measurement starts in the native event handler. Fixtures are metadata, not real copied files. Warmup: 1 second; first initialization and memory remain in wall-time/peak.",
        }
    }
}
#[cfg(windows)]
fn native_process_metrics() -> (f64, f64, f64) {
    use windows::Win32::{
        Foundation::FILETIME,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
    };
    unsafe {
        let process = GetCurrentProcess();
        let mut memory = PROCESS_MEMORY_COUNTERS_EX::default();
        memory.cb = std::mem::size_of_val(&memory) as u32;
        let _ = GetProcessMemoryInfo(process, &mut memory as *mut _ as *mut _, memory.cb);
        let mut created = FILETIME::default();
        let mut exit = created;
        let mut kernel = created;
        let mut user = created;
        let _ = GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user);
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        (
            memory.PeakWorkingSetSize as f64 / 1048576.,
            memory.PrivateUsage as f64 / 1048576.,
            (ticks(kernel) + ticks(user)) as f64 / 10_000_000.,
        )
    }
}
