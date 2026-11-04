#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod layout;
mod metrics;
mod model;
mod paint;
mod selectable;
mod selection;
mod selection_smoke;
mod session;
mod task_panel;
mod theme;

use gpui::{prelude::*, *};
use metrics::{Metrics, Options};
use model::{Detail, Event, PageCache, Selection, Source};
use paint::{
    FileState, FlowCache, GridTextCache, SharedFlowCache, SharedGridTextCache, file_background,
    grid_name, icon, tooltip,
};
use selectable::Selectable;
use selection::{GridBand, SelectionWorker};
use std::{
    cell::RefCell,
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use task_panel::{ClosePanel, TaskPanel};
use theme::{Palette, Role as ColorRole};

struct Backleaf {
    colors: &'static Palette,
    session_locked: bool,
    _session: session::SessionMonitor,
    grid: Entity<FileGrid>,
    options: Options,
    source: Source,
    path: PathBuf,
    history: Vec<PathBuf>,
    generation: u64,
    tx: async_channel::Sender<Event>,
    cache: PageCache,
    labels: SharedGridTextCache,
    requested: Range<usize>,
    serial: u64,
    selection: Selection,
    selection_worker: SelectionWorker,
    marquee: Option<Marquee>,
    keyboard_focus: bool,
    hovered_id: Option<usize>,
    pressed_id: Option<usize>,
    suppress_click: bool,
    cell_width: f32,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    columns: usize,
    origin_row: usize,
    details: Option<Detail>,
    detail_name: Option<Entity<Selectable>>,
    detail_path: Option<Entity<Selectable>>,
    detail_token: u64,
    detail_pending: bool,
    open_pending: bool,
    error: Option<String>,
    panel: bool,
    task: Entity<TaskPanel>,
    flow_cache: SharedFlowCache,
    clock_pending: bool,
    benchmark_step: usize,
    last_motion_frame: u128,
    metrics: Rc<RefCell<Metrics>>,
    _subscriptions: Vec<Subscription>,
}
struct FileGrid {
    owner: WeakEntity<Backleaf>,
    _observer: Subscription,
}
struct Marquee {
    base: Selection,
    anchor: (f64, f64),
    start: Point<Pixels>,
    pointer: Point<Pixels>,
    additive: bool,
    active: bool,
    held: bool,
    band: GridBand,
    token: u64,
    accepted: u64,
    last_tick: Instant,
}
impl Drop for Backleaf {
    fn drop(&mut self) {
        self.write_report();
    }
}
impl Render for FileGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner
            .upgrade()
            .expect("grid owner")
            .update(cx, |this, cx| this.render_grid(window, cx))
    }
}
impl Backleaf {
    fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let colors = options
            .theme
            .map(theme::palette)
            .unwrap_or_else(|| theme::for_window(window));
        configure_native_corners(window, colors.dark);
        let owner = cx.entity();
        let grid = cx.new(|cx| FileGrid {
            owner: owner.downgrade(),
            _observer: cx.observe(&owner, |_, _, cx| cx.notify()),
        });
        let (tx, rx) = async_channel::bounded(8);
        let (selection_tx, selection_rx) = async_channel::bounded(1);
        let selection_worker = SelectionWorker::new(selection_tx);
        cx.spawn(async move |this, cx| {
            while let Ok(result) = selection_rx.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if let Some(m) = this.marquee.as_mut()
                            && m.token == result.token
                        {
                            this.selection.mask = result.mask;
                            this.selection.focus =
                                m.band
                                    .last_id()
                                    .or(if m.additive { m.base.focus } else { None });
                            if !m.additive {
                                this.selection.anchor = m.band.first_id().unwrap_or(0);
                            }
                            m.accepted = result.token;
                            if !m.held {
                                this.marquee = None;
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let session = session::monitor(tx.clone());
        let path = options.path.clone().unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:\\"))
        });
        let source = if let Some(count) = options.fixture {
            Source::fixture(count)
        } else {
            Source::directory(path.clone(), 1, tx.clone())
        };
        let focus = cx.focus_handle().tab_stop(true);
        window.focus(&focus, cx);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if this.update(cx, |this, cx| this.receive(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let visibility = cx.observe_window_visibility(window, |this, visibility, _, cx| {
            if !visibility.is_visible() {
                this.finish_marquee(cx);
                this.pressed_id = None;
                this.hovered_id = None;
            }
            let mut m = this.metrics.borrow_mut();
            let event = format!(
                "{} requests={} frames={}",
                if visibility.is_visible() {
                    "visible"
                } else {
                    "hidden"
                },
                m.requests,
                m.cpu.len()
            );
            m.event(&event);
            m.last_frame = None;
            drop(m);
            cx.notify();
        });
        if options.output.is_some() {
            let seconds = options.seconds;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(seconds))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.write_report();
                    cx.quit();
                });
            })
            .detach();
        }
        let panel = options.fixture.is_some();
        let flow_cache = Rc::new(RefCell::new(FlowCache::default()));
        let metrics = Rc::new(RefCell::new(Metrics::new()));
        let task = cx.new(|_| TaskPanel {
            colors,
            expanded: true,
            active: options.flow,
            reduced: options.reduced_motion,
            started: Instant::now(),
            cache: flow_cache.clone(),
            metrics: metrics.clone(),
        });
        let closed = cx.subscribe(&task, |this, _, _: &ClosePanel, cx| {
            this.panel = false;
            cx.notify();
        });
        let activated = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.finish_marquee(cx);
                this.pressed_id = None;
                this.hovered_id = None;
            }
            cx.notify();
        });
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.options.theme.is_some() {
                return;
            }
            let colors = theme::for_window(window);
            this.colors = colors;
            this.labels.borrow_mut().clear();
            configure_native_corners(window, colors.dark);
            this.task.update(cx, |task, cx| {
                task.colors = colors;
                task.cache.borrow_mut().clear();
                cx.notify();
            });
            for text in [this.detail_name.as_ref(), this.detail_path.as_ref()]
                .into_iter()
                .flatten()
            {
                text.update(cx, |text, cx| {
                    text.colors = colors;
                    cx.notify();
                });
            }
            cx.notify();
        });
        if options.selection_smoke {
            selection_smoke::schedule(window, cx);
        }
        Self {
            colors,
            session_locked: true,
            _session: session,
            grid,
            options,
            source,
            path,
            history: vec![],
            generation: 1,
            tx,
            cache: PageCache::new(),
            labels: Rc::new(RefCell::new(GridTextCache::default())),
            requested: 0..0,
            serial: 0,
            selection: Selection::default(),
            selection_worker,
            marquee: None,
            keyboard_focus: false,
            hovered_id: None,
            pressed_id: None,
            suppress_click: false,
            cell_width: layout::LAYOUT.grid.minimum_cell_width,
            focus,
            scroll: UniformListScrollHandle::new(),
            columns: 1,
            origin_row: 0,
            details: None,
            detail_name: None,
            detail_path: None,
            detail_token: 0,
            detail_pending: false,
            open_pending: false,
            error: None,
            panel,
            task,
            flow_cache,
            clock_pending: false,
            benchmark_step: 0,
            last_motion_frame: 0,
            metrics,
            _subscriptions: vec![visibility, closed, activated, appearance],
        }
    }
    fn write_report(&self) {
        if let Some(path) = &self.options.output {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut report = self.metrics.borrow().report(
                &self.options,
                self.source.count(),
                self.flow_cache.borrow().misses,
                (self.labels.borrow().builds, self.labels.borrow().peak),
            );
            report.selected_count = self.selection.count();
            report.selection_bitmap_serialized_bytes =
                self.selection.mask.bitmap_serialized_bytes();
            report.selection_pending = self.selection_busy();
            report.theme_dark = self.colors.dark;
            if let Ok(json) = serde_json::to_vec_pretty(&report) {
                let _ = std::fs::write(path, json);
            }
        }
    }
    fn receive(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Session { locked } => {
                if locked {
                    self.finish_marquee(cx);
                }
                self.session_locked = locked;
                let mut m = self.metrics.borrow_mut();
                if m.session_initially_locked.is_none() {
                    m.session_initially_locked = Some(locked);
                }
                m.session_locked = locked;
                let event = format!(
                    "session-{} requests={} frames={}",
                    if locked { "locked" } else { "unlocked" },
                    m.requests,
                    m.cpu.len()
                );
                m.event(&event);
                m.last_frame = None;
                drop(m);
                cx.notify();
            }
            Event::Count {
                generation,
                count,
                done,
                error,
            } if generation == self.generation => {
                if let Source::Directory {
                    count: current,
                    done: finished,
                    ..
                } = &mut self.source
                {
                    *current = (*current).max(count);
                    *finished = done;
                }
                if error.is_some() {
                    self.error = error;
                }
                cx.notify();
            }
            Event::Page {
                generation,
                serial,
                items,
            } if generation == self.generation && serial == self.serial => {
                self.cache.retain(|id, _| self.requested.contains(id));
                for item in items {
                    if self.requested.contains(&item.id) {
                        self.cache.insert(item.id, Arc::new(item));
                    }
                }
                let mut m = self.metrics.borrow_mut();
                m.cache = m.cache.max(self.cache.len());
                drop(m);
                cx.notify();
            }
            Event::Detail {
                generation,
                token,
                value,
            } if generation == self.generation && token == self.detail_token => {
                self.detail_pending = false;
                match value {
                    Ok(detail) => self.accept_detail(detail, cx),
                    Err(error) => {
                        self.error = Some(error);
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
    }
    fn accept_detail(&mut self, detail: Detail, cx: &mut Context<Self>) {
        if self.open_pending {
            self.open_pending = false;
            if detail.directory {
                self.navigate(detail.path, true, cx);
            } else {
                cx.open_with_system(&detail.path);
            }
        } else {
            self.metrics
                .borrow_mut()
                .event(&format!("detail-ready id={}", detail.id));
            self.detail_name =
                Some(cx.new(|cx| Selectable::new(detail.name.clone(), self.colors, cx)));
            self.detail_path = Some(cx.new(|cx| {
                Selectable::new(detail.path.to_string_lossy().into_owned(), self.colors, cx)
            }));
            self.details = Some(detail);
            cx.notify();
        }
    }
    fn navigate(&mut self, path: PathBuf, push: bool, cx: &mut Context<Self>) {
        self.cancel_marquee();
        if self.options.fixture.is_some() {
            return;
        }
        if push {
            self.history.push(self.path.clone());
        }
        self.generation += 1;
        self.detail_token += 1;
        self.details = None;
        self.detail_name = None;
        self.detail_path = None;
        self.detail_pending = false;
        self.path = path;
        self.cache.clear();
        self.labels.borrow_mut().clear();
        self.hovered_id = None;
        self.pressed_id = None;
        self.requested = 0..0;
        self.selection = Selection::default();
        self.error = None;
        self.scroll = UniformListScrollHandle::new();
        self.origin_row = 0;
        self.source = Source::directory(self.path.clone(), self.generation, self.tx.clone());
        cx.notify();
    }
    fn request_detail(&mut self, id: usize, open: bool, cx: &mut Context<Self>) {
        self.panel = false;
        self.detail_token += 1;
        self.detail_pending = true;
        self.open_pending = open && self.options.fixture.is_none();
        if let Some(detail) = self.source.detail(self.detail_token, id) {
            self.detail_pending = false;
            self.accept_detail(detail, cx);
        }
        cx.notify();
    }
    fn ensure_page(&mut self, range: Range<usize>) {
        if self.requested == range {
            return;
        }
        self.requested = range.clone();
        self.serial += 1;
        self.cache.retain(|id, _| range.contains(id));
        self.labels.borrow_mut().retain(&range);
        if self.options.fixture.is_some() {
            for id in range {
                self.cache
                    .entry(id)
                    .or_insert_with(|| Arc::new(model::fixture_item(id)));
            }
        } else {
            self.source.request(self.serial, range);
        }
        let mut m = self.metrics.borrow_mut();
        m.cache = m.cache.max(self.cache.len());
    }
    fn grid_width(&self, window: &Window) -> f32 {
        f32::from(window.viewport_size().width) - if self.details.is_some() { 320. } else { 0. }
    }
    fn global_point(&self, p: Point<Pixels>) -> (f64, f64) {
        let offset = -f32::from(self.scroll.0.borrow().base_handle.offset().y) as f64;
        (
            f32::from(p.x) as f64 - 12.,
            self.origin_row as f64 * layout::row_step() as f64 + offset + f32::from(p.y) as f64
                - 60.,
        )
    }
    fn hit_file(&self, p: Point<Pixels>, window: &Window) -> Option<usize> {
        let (x, y) = self.global_point(p);
        if x < 0.
            || y < 0.
            || f32::from(p.x) >= self.grid_width(window) - 12.
            || f32::from(p.y) < 60.
            || f32::from(p.y) > f32::from(window.viewport_size().height) - 44.
        {
            return None;
        }
        let col =
            (x / (self.cell_width as f64 + layout::LAYOUT.grid.column_gap as f64)).floor() as usize;
        let row = (y / layout::row_step() as f64).floor() as usize;
        if col >= self.columns {
            return None;
        }
        let id = row * self.columns + col;
        if id >= self.source.count() {
            return None;
        }
        let dx = x - col as f64 * (self.cell_width as f64 + layout::LAYOUT.grid.column_gap as f64);
        let dy = y - row as f64 * layout::row_step() as f64;
        (dx < self.cell_width as f64 && dy < layout::LAYOUT.grid.cell_height as f64).then_some(id)
    }
    fn selection_busy(&self) -> bool {
        self.marquee
            .as_ref()
            .is_some_and(|m| m.active && !m.held && m.accepted != m.token)
    }
    fn is_selected(&self, id: usize) -> bool {
        if let Some(m) = &self.marquee
            && m.active
        {
            m.band.contains(id) || (m.additive && m.base.contains(id))
        } else {
            self.selection.contains(id)
        }
    }
    fn cancel_marquee(&mut self) {
        self.selection_worker.cancel();
        if let Some(m) = self.marquee.take() {
            self.selection = m.base;
        }
    }
    fn begin_marquee(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection_busy() {
            return;
        }
        self.cancel_marquee();
        self.suppress_click = false;
        self.keyboard_focus = false;
        if let Some(id) = self.hit_file(event.position, window) {
            self.pressed_id = Some(id);
            cx.notify();
            return;
        }
        window.focus(&self.focus, cx);
        let base = self.selection.clone();
        let additive = event.modifiers.control || event.modifiers.shift;
        if !additive {
            self.selection = Selection::default();
        }
        self.marquee = Some(Marquee {
            base,
            anchor: self.global_point(event.position),
            start: event.position,
            pointer: event.position,
            additive,
            active: false,
            held: true,
            band: GridBand::default(),
            token: 0,
            accepted: 0,
            last_tick: Instant::now(),
        });
        cx.notify();
    }
    fn update_marquee_band(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(m) = &self.marquee else {
            return;
        };
        if !m.active || !m.held {
            return;
        }
        let pointer = point(
            m.pointer.x.clamp(px(0.), px(self.grid_width(window))),
            m.pointer
                .y
                .clamp(px(44.), window.viewport_size().height - px(28.)),
        );
        let band = GridBand::from_rect(
            self.columns,
            self.cell_width as f64,
            self.source.count(),
            m.anchor,
            self.global_point(pointer),
        );
        let m = self.marquee.as_mut().unwrap();
        if band != m.band {
            m.band = band;
            m.token = self
                .selection_worker
                .request(m.base.mask.clone(), band, m.additive);
        }
        cx.notify();
    }
    fn move_marquee(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.marquee.as_mut() else {
            return;
        };
        if !m.held {
            return;
        }
        if !event.dragging() {
            self.finish_marquee(cx);
            return;
        }
        m.pointer = event.position;
        let delta = event.position - m.start;
        if f32::from(delta.x).hypot(f32::from(delta.y)) >= 4. {
            m.active = true;
        }
        self.update_marquee_band(window, cx);
    }
    fn finish_marquee(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.marquee.as_mut() else {
            return;
        };
        m.held = false;
        if m.active {
            self.suppress_click = true;
        }
        if !m.active || m.accepted == m.token {
            self.marquee = None;
        }
        cx.notify();
    }
    fn marquee_velocity(&self, window: &Window) -> f32 {
        let Some(m) = &self.marquee else {
            return 0.;
        };
        if !m.active || !m.held || !window.is_window_active() {
            return 0.;
        }
        let top = self.global_point(point(px(12.), px(60.))).1;
        let visible = (f32::from(window.viewport_size().height) - 104.).max(1.);
        let maximum = (self.source.count().div_ceil(self.columns) as f64
            * layout::row_step() as f64
            - visible as f64)
            .max(0.);
        let y = f32::from(m.pointer.y);
        let bottom = f32::from(window.viewport_size().height) - 44.;
        if y < 84. && top > 0. {
            -900. * ((84. - y) / 24.).clamp(0., 1.)
        } else if y > bottom - 24. && top < maximum {
            900. * ((y - bottom + 24.) / 24.).clamp(0., 1.)
        } else {
            0.
        }
    }
    fn step_marquee(&mut self, window: &Window, cx: &mut Context<Self>) {
        let speed = self.marquee_velocity(window);
        if speed == 0. {
            return;
        }
        let m = self.marquee.as_mut().unwrap();
        let now = Instant::now();
        let delta = now.duration_since(m.last_tick).as_secs_f32().min(0.05);
        m.last_tick = now;
        {
            let state = self.scroll.0.borrow();
            let handle = &state.base_handle;
            let maximum = (self.source.count().div_ceil(self.columns).min(4096) as f32
                * layout::row_step()
                - (f32::from(window.viewport_size().height) - 104.))
                .max(0.);
            let y = (f32::from(handle.offset().y) - speed * delta).clamp(-maximum, 0.);
            handle.set_offset(point(px(0.), px(y)));
        }
        self.update_marquee_band(window, cx);
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        self.metrics.borrow_mut().input("key");
        if key == "tab" && !modifiers.control && !modifiers.alt {
            self.keyboard_focus = true;
            if modifiers.shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "escape" && self.marquee.is_some() {
            self.cancel_marquee();
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        }
        if self.selection_busy() {
            return;
        }
        if self.marquee.is_some() {
            self.cancel_marquee();
        }
        self.keyboard_focus = true;
        if key == "escape" {
            window.focus(&self.focus, cx);
            if self.details.take().is_some() || self.detail_pending {
                self.detail_token += 1;
                self.detail_pending = false;
                self.detail_name = None;
                self.detail_path = None;
            } else if self.panel {
                self.panel = false;
            } else {
                let focus = self.selection.focus;
                self.selection = Selection::default();
                self.selection.focus = focus;
                self.selection.anchor = focus.unwrap_or(0);
            }
            cx.notify();
            return;
        }
        if key == "f6" && self.options.fixture.is_some() {
            self.panel = !self.panel;
            cx.notify();
            return;
        }
        if modifiers.alt && key == "left" {
            if let Some(path) = self.history.pop() {
                self.navigate(path, false, cx);
            }
            return;
        }
        if modifiers.alt && key == "up" {
            if let Some(parent) = self.path.parent() {
                self.navigate(parent.to_owned(), true, cx);
            }
            return;
        }
        if !self.focus.is_focused(window) {
            return;
        }
        if modifiers.control && key == "a" {
            self.selection.all(self.source.count());
            cx.notify();
            return;
        }
        if key == "enter" {
            if let Some(id) = self.selection.focus {
                self.request_detail(id, !modifiers.alt, cx);
            }
            return;
        }
        if self.source.count() == 0 {
            return;
        }
        let current = self.selection.focus.unwrap_or(0);
        if key == "space" && !modifiers.control {
            self.selection
                .select(current, !modifiers.shift, modifiers.shift);
            cx.notify();
            return;
        }
        let columns = self.columns;
        let page = columns
            * ((f32::from(window.viewport_size().height) - 104.) / layout::row_step())
                .floor()
                .max(1.) as usize;
        let next = match key {
            "right" => Some(current.saturating_add(1)),
            "left" => Some(current.saturating_sub(1)),
            "down" => Some(current.saturating_add(columns)),
            "up" => Some(current.saturating_sub(columns)),
            "home" => Some(0),
            "end" => Some(self.source.count() - 1),
            "pagedown" => Some(current.saturating_add(page)),
            "pageup" => Some(current.saturating_sub(page)),
            _ => None,
        };
        if let Some(next) = next {
            let next = if self.selection.focus.is_none() && key != "end" {
                0
            } else {
                next.min(self.source.count() - 1)
            };
            if modifiers.control && !modifiers.shift {
                self.selection.focus = Some(next);
            } else {
                self.selection
                    .select(next, modifiers.control, modifiers.shift);
            }
            self.scroll_global(next / columns, ScrollStrategy::Nearest, false);
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }
    fn tile(
        &mut self,
        id: usize,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = self.cache.get(&id).cloned();
        let name = item.as_ref().map(|x| x.name.clone()).unwrap_or_default();
        let label = item.as_ref().map(|x| x.label.clone()).unwrap_or_default();
        let folder = item.as_ref().is_some_and(|x| x.directory);
        let active = window.is_window_active();
        let focused = self.selection.focus == Some(id);
        let grid_focused = self.focus.is_focused(window);
        let state = FileState {
            selected: self.is_selected(id),
            active,
            focused: focused && self.keyboard_focus && grid_focused,
            hovered: active && self.hovered_id == Some(id) && self.marquee.is_none(),
            pressed: active && self.pressed_id == Some(id),
        };
        let colors = self.colors;
        let l = &layout::LAYOUT;
        div()
            .id(SharedString::from(format!("file-{}-{id}", self.generation)))
            .role(Role::ListBoxOption)
            .aria_label(name.clone())
            .aria_selected(state.selected)
            .when(focused && grid_focused, |d| d.aria_active_descendant())
            .relative()
            .w(px(width))
            .h(px(l.grid.cell_height))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(l.file.icon_name_gap))
            .px(px(l.file.visual_padding_x))
            .pt(px(l.file.visual_padding_top))
            .pb(px(l.file.visual_padding_bottom))
            .when(self.marquee.is_none(), |d| {
                d.tooltip(tooltip(name.clone(), colors))
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                let next = if *hovered {
                    Some(id)
                } else if this.hovered_id == Some(id) {
                    None
                } else {
                    this.hovered_id
                };
                if next != this.hovered_id {
                    this.hovered_id = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if this.selection_busy()
                    || (event.mouse_position().is_some()
                        && (this.suppress_click
                            || this.hit_file(event.position(), window) != Some(id)))
                {
                    return;
                }
                this.metrics.borrow_mut().input("pointer-select");
                this.keyboard_focus = event.mouse_position().is_none();
                window.focus(&this.focus, cx);
                let m = event.modifiers();
                this.selection.select(id, m.control, m.shift);
                if event.click_count() == 2 {
                    this.request_detail(id, true, cx);
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.selection_busy() || this.hit_file(event.position, window) != Some(id) {
                        return;
                    }
                    if !this.selection.contains(id) {
                        this.selection.select(id, false, false);
                    }
                    window.focus(&this.focus, cx);
                    this.request_detail(id, false, cx);
                }),
            )
            .when(item.is_some(), |d| {
                d.child(file_background(
                    id,
                    label.clone(),
                    state,
                    self.labels.clone(),
                    colors,
                ))
            })
            .child(
                img(match (folder, colors.dark) {
                    (true, false) => "folder.svg",
                    (true, true) => "folder-dark.svg",
                    (false, false) => "file.svg",
                    (false, true) => "file-dark.svg",
                })
                .size(px(l.grid.icon_size))
                .flex_none()
                .when(item.is_none(), |s| s.opacity(0.2)),
            )
            .child(
                div()
                    .w_full()
                    .h(px(36.))
                    .flex_none()
                    .when(item.is_some(), |d| {
                        d.child(grid_name(id, label, state, self.labels.clone(), colors))
                    }),
            )
            .into_any_element()
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let colors = self.colors;
        let action = Rc::new(click);
        let keyboard = action.clone();
        div()
            .id(id)
            .size(px(layout::LAYOUT.control.hit_size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(layout::LAYOUT.control.radius))
            .border_2()
            .border_color(transparent_black())
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .tooltip(tooltip(label.to_owned(), colors))
            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
            .active(move |d| d.bg(colors.color(ColorRole::Pressed)))
            .focus_visible(move |d| d.border_color(colors.color(ColorRole::Focus)))
            .on_key_down(move |event, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    keyboard(&ClickEvent::default(), window, cx);
                }
            })
            .on_click(move |event, window, cx| action(event, window, cx))
            .child(icon(id, colors))
    }
    fn titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let title = if let Some(count) = self.options.fixture {
            format!("渲染基准 · {} 项", number(count))
        } else {
            self.path
                .file_name()
                .unwrap_or(self.path.as_os_str())
                .to_string_lossy()
                .into_owned()
        };
        div()
            .h(px(44.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .bg(colors.color(ColorRole::Chrome))
            .child(
                div()
                    .w(px(8.))
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(self.button(
                "back",
                "后退",
                cx.listener(|this, _, _, cx| {
                    if let Some(path) = this.history.pop() {
                        this.navigate(path, false, cx);
                    }
                }),
            ))
            .child(self.button(
                "up",
                "上一级",
                cx.listener(|this, _, _, cx| {
                    if let Some(path) = this.path.parent() {
                        this.navigate(path.to_owned(), true, cx);
                    }
                }),
            ))
            .child(
                div()
                    .id("path")
                    .min_w_0()
                    .max_w(px(600.))
                    .px(px(8.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .tooltip(tooltip(self.path.to_string_lossy().into_owned(), colors))
                    .child(div().truncate().child(title)),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            .children(
                [
                    ("min", "最小化", WindowControlArea::Min),
                    ("max", "最大化或还原", WindowControlArea::Max),
                    ("close", "关闭", WindowControlArea::Close),
                ]
                .into_iter()
                .map(|(name, label, area)| {
                    div()
                        .id(name)
                        .w(px(46.))
                        .h_full()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .role(Role::Button)
                        .aria_label(label)
                        .window_control_area(area)
                        .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                        .on_click(move |_, window, _| match area {
                            WindowControlArea::Min => window.minimize_window(),
                            WindowControlArea::Max => window.zoom_window(),
                            WindowControlArea::Close => window.remove_window(),
                            _ => {}
                        })
                        .child(icon(name, colors))
                }),
            )
            .into_any_element()
    }
    fn scroll_global(&mut self, row: usize, strategy: ScrollStrategy, strict: bool) {
        let total = self.source.count().div_ceil(self.columns).max(1);
        let row = row.min(total - 1);
        let span = total.min(4096);
        if row < self.origin_row || row >= self.origin_row + span {
            self.origin_row = row.saturating_sub(span / 2).min(total.saturating_sub(span));
        }
        let local = row - self.origin_row;
        if strict {
            self.scroll.scroll_to_item_strict(local, strategy);
        } else {
            self.scroll.scroll_to_item(local, strategy);
        }
    }
    fn arm_clock(&mut self, window: &Window, cx: &mut Context<Self>) {
        let animate = !self.session_locked
            && self.panel
            && self.task.read(cx).expanded
            && self.options.flow > 0
            && !self.options.reduced_motion
            && !cx.reduce_motion()
            && window.is_visible();
        let scripted = !self.session_locked
            && self.options.output.is_some()
            && self.options.scroll
            && window.is_visible();
        let selecting =
            !self.session_locked && window.is_visible() && self.marquee_velocity(window) != 0.;
        if self.clock_pending || !(animate || scripted || selecting) {
            return;
        }
        self.clock_pending = true;
        {
            let mut m = self.metrics.borrow_mut();
            m.requests += 1;
            m.active_clocks += 1;
            m.max_clocks = m.max_clocks.max(m.active_clocks);
            if !window.is_visible() {
                m.hidden_requests += 1;
            }
        }
        let this = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = this.update(cx, |this, cx| {
                this.clock_pending = false;
                {
                    let mut m = this.metrics.borrow_mut();
                    m.active_clocks = m.active_clocks.saturating_sub(1);
                }
                if window.is_visible() && !this.session_locked {
                    this.step_marquee(window, cx);
                    let motion = this.panel
                        && this.task.read(cx).expanded
                        && this.options.flow > 0
                        && !this.options.reduced_motion
                        && !cx.reduce_motion();
                    let motion_frame = this.task.read(cx).started.elapsed().as_nanos() / 16_666_667;
                    if motion && motion_frame != this.last_motion_frame {
                        this.last_motion_frame = motion_frame;
                        this.task.update(cx, |_, cx| cx.notify());
                    }
                    if this.options.scroll && this.options.output.is_some() {
                        this.benchmark_step += 1;
                        let step = this.benchmark_step;
                        let rows = this.source.count().div_ceil(this.columns).max(1);
                        let row = if step % 180 < 150 {
                            (step * 2) % rows
                        } else {
                            step.wrapping_mul(104729) % rows
                        };
                        this.scroll_global(row, ScrollStrategy::Top, true);
                        cx.notify();
                    }
                }
                this.arm_clock(window, cx);
            });
        });
    }
    fn render_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.metrics.borrow_mut().grid_builds += 1;
        let width = f32::from(window.viewport_size().width);
        let gap = layout::LAYOUT.grid.column_gap;
        let grid_width = (width - if self.details.is_some() { 320. } else { 0. } - 24.)
            .max(layout::LAYOUT.grid.minimum_cell_width);
        let columns = ((grid_width + gap) / (layout::LAYOUT.grid.minimum_cell_width + gap))
            .floor()
            .max(1.) as usize;
        if self.columns != columns {
            self.cancel_marquee();
            self.hovered_id = None;
            self.pressed_id = None;
            let anchor = self.selection.focus.unwrap_or_else(|| {
                (self.origin_row
                    + (-f32::from(self.scroll.0.borrow().base_handle.offset().y)
                        / layout::row_step())
                    .floor() as usize)
                    * self.columns
            });
            self.columns = columns;
            self.scroll_global(anchor / columns, ScrollStrategy::Top, true);
        }
        let cell_width = ((grid_width - gap * (columns - 1) as f32) / columns as f32).max(1.);
        self.cell_width = cell_width;
        let geometry = (cell_width.to_bits(), window.scale_factor().to_bits());
        {
            let mut labels = self.labels.borrow_mut();
            if labels.geometry != geometry {
                labels.clear();
                labels.geometry = geometry;
            }
        }
        let count = self.source.count();
        let total_rows = count.div_ceil(columns);
        let rows = total_rows.min(4096);
        self.origin_row = self.origin_row.min(total_rows.saturating_sub(rows));
        // GPUI's scroll offset is f32. Keep it below 524,288 px even at 10M items.
        // Rebase around the same file + sub-row offset before absolute positions lose precision.
        let offset = -f32::from(self.scroll.0.borrow().base_handle.offset().y);
        let local_row = (offset / layout::row_step()).floor().max(0.) as usize;
        if self.scroll.0.borrow().deferred_scroll_to_item.is_none()
            && rows == 4096
            && !(128..=3968).contains(&local_row)
        {
            let global = self.origin_row + local_row;
            let origin = global
                .saturating_sub(2048)
                .min(total_rows.saturating_sub(rows));
            if origin != self.origin_row {
                let fraction = offset - local_row as f32 * layout::row_step();
                self.origin_row = origin;
                self.scroll.0.borrow().base_handle.set_offset(point(
                    px(0.),
                    px(-((global - origin) as f32 * layout::row_step() + fraction)),
                ));
            }
        }
        let origin = self.origin_row;
        let visible_height = f32::from(window.viewport_size().height) - 104.;
        div()
            .id("file-viewport")
            .role(Role::ListBox)
            .aria_label("文件")
            .size_full()
            .px(px(12.))
            .py(px(16.))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_marquee))
            .child(
                uniform_list(
                    "file-grid",
                    rows,
                    cx.processor(move |this, range: Range<usize>, window, cx| {
                        // GPUI asks for a single row twice to measure its fixed geometry.
                        // That probe must never replace the real viewport's page/cache.
                        if range.len() == 1 && rows > 1 && visible_height > layout::row_step() {
                            return vec![div().h(px(layout::row_step())).w_full()];
                        }
                        let prefetch = (origin + range.start.saturating_sub(2)) * columns
                            ..((origin + range.end + 2) * columns).min(count);
                        this.ensure_page(prefetch);
                        {
                            let mut m = this.metrics.borrow_mut();
                            m.cells = m.cells.max(range.len() * columns);
                        }
                        range
                            .map(|row| row + origin)
                            .map(|row| {
                                div()
                                    .h(px(layout::row_step()))
                                    .w_full()
                                    .flex()
                                    .gap(px(layout::LAYOUT.grid.column_gap))
                                    .children(
                                        (row * columns..((row + 1) * columns).min(count))
                                            .map(|id| this.tile(id, cell_width, window, cx))
                                            .collect::<Vec<_>>(),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h_full()
                .w_full()
                .with_width_from_item(Some(local_row.min(rows.saturating_sub(1))))
                .track_scroll(&self.scroll),
            )
            .into_any_element()
    }
    fn details_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let detail = self.details.as_ref().unwrap();
        let name = detail.name.clone();
        let path = detail.path.to_string_lossy().into_owned();
        div()
            .id("details")
            .w(px(320.))
            .flex_none()
            .h_full()
            .border_l_1()
            .border_color(self.colors.color(ColorRole::Border))
            .px(px(12.))
            .py(px(16.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("详情")
                    .child(self.button(
                        "close",
                        "关闭详情",
                        cx.listener(|this, _, window, cx| {
                            this.details = None;
                            this.detail_name = None;
                            this.detail_path = None;
                            window.focus(&this.focus, cx);
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .id("detail-body")
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .id("detail-name-field")
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(12.))
                                    .text_color(self.colors.color(ColorRole::Secondary))
                                    .child("名称")
                                    .child(self.button(
                                        "copy",
                                        "复制完整名称",
                                        move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                name.clone(),
                                            ))
                                        },
                                    )),
                            )
                            .child(self.detail_name.clone().unwrap()),
                    )
                    .child(
                        div()
                            .id("detail-path-field")
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(12.))
                                    .text_color(self.colors.color(ColorRole::Secondary))
                                    .child("位置")
                                    .child(self.button(
                                        "copy",
                                        "复制完整路径",
                                        move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                path.clone(),
                                            ))
                                        },
                                    )),
                            )
                            .child(self.detail_path.clone().unwrap()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(self.colors.color(ColorRole::Secondary))
                            .child(if detail.directory {
                                "文件夹".into()
                            } else {
                                detail
                                    .bytes
                                    .map(|n| format!("{} 字节", number(n as usize)))
                                    .unwrap_or_else(|| "—".into())
                            }),
                    ),
            )
            .into_any_element()
    }
}
impl Render for Backleaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        self.metrics.borrow_mut().accessibility_active |= window.is_a11y_active();
        self.metrics
            .borrow_mut()
            .begin([width, height], window.scale_factor());
        let count = self.source.count();
        self.arm_clock(window, cx);
        let status = if let Some(m) = &self.marquee
            && m.active
            && (m.accepted != m.token || m.held)
        {
            if m.additive && m.accepted != m.token {
                "正在选择…".into()
            } else {
                format!(
                    "已选 {} 项",
                    number(if m.additive {
                        self.selection.count()
                    } else {
                        m.band.len()
                    })
                )
            }
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.selection.count() > 0 {
            format!("已选 {} 项", number(self.selection.count()))
        } else {
            format!("{} 项", number(count))
        };
        let marker = self.metrics.clone();
        let owner = cx.entity().downgrade();
        let overlay = self
            .marquee
            .as_ref()
            .filter(|m| m.active && m.held)
            .map(|m| {
                let top = self.global_point(point(px(12.), px(60.))).1;
                let a = point(
                    px((m.anchor.0 + 12.) as f32),
                    px((m.anchor.1 - top + 60.) as f32),
                );
                let left = f32::from(a.x.min(m.pointer.x)).clamp(0., self.grid_width(window));
                let right = f32::from(a.x.max(m.pointer.x)).clamp(0., self.grid_width(window));
                let top = f32::from(a.y.min(m.pointer.y)).clamp(44., height - 28.);
                let bottom = f32::from(a.y.max(m.pointer.y)).clamp(44., height - 28.);
                Bounds::new(
                    point(px(left), px(top)),
                    size(px(right - left), px(bottom - top)),
                )
            });
        div()
            .id("backleaf")
            .role(Role::Application)
            .aria_label("拾径 Backleaf")
            .track_focus(&self.focus)
            .tab_stop(true)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.color(ColorRole::Canvas))
            .font_family("Microsoft YaHei UI")
            .text_size(px(13.))
            .text_color(colors.color(ColorRole::Primary))
            .on_key_down(cx.listener(Self::key))
            .on_scroll(cx.listener(|this, _, window, cx| {
                this.metrics.borrow_mut().input("wheel");
                this.hovered_id = None;
                this.update_marquee_band(window, cx);
                this.grid.update(cx, |_, cx| cx.notify());
            }))
            .child(self.titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(if window.is_a11y_active() {
                        div()
                            .w(px(width - if self.details.is_some() { 320. } else { 0. }))
                            .h(px((height - 72.).max(0.)))
                            .child(self.grid.clone())
                            .into_any_element()
                    } else {
                        self.grid
                            .clone()
                            .cached(
                                StyleRefinement::default()
                                    .w(px(width - if self.details.is_some() { 320. } else { 0. }))
                                    .h(px((height - 72.).max(0.))),
                            )
                            .into_any_element()
                    })
                    .when(self.details.is_some(), |d| d.child(self.details_panel(cx))),
            )
            .child(
                div()
                    .w_full()
                    .h(px(28.))
                    .flex_none()
                    .px(px(12.))
                    .text_size(px(11.))
                    .text_color(self.colors.color(ColorRole::Secondary))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id("status-text")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .role(Role::Label)
                            .aria_value(status.clone())
                            .tooltip(tooltip(status.clone(), colors))
                            .child(status),
                    ),
            )
            .when(self.panel, |d| {
                d.child(
                    div()
                        .absolute()
                        .right(px(12.))
                        .bottom(px(36.))
                        .w(px(400.))
                        .max_w_full()
                        .child(self.task.clone()),
                )
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _cx| {
                        if let Some(bounds) = overlay {
                            window.paint_quad(quad(
                                bounds,
                                px(0.),
                                colors.color(ColorRole::Focus).opacity(0.08),
                                px(1.),
                                colors.color(ColorRole::Focus),
                                BorderStyle::Solid,
                            ));
                        }
                        let downer = owner.clone();
                        window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = downer.upgrade()
                                && view.read(cx).keyboard_focus
                            {
                                view.update(cx, |this, cx| {
                                    this.keyboard_focus = false;
                                    cx.notify();
                                });
                            }
                        });
                        let mover = owner.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = mover.upgrade()
                                && view.read(cx).marquee.is_some()
                            {
                                view.update(cx, |this, cx| this.move_marquee(event, window, cx));
                            }
                        });
                        let upper = owner.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && event.button == MouseButton::Left
                                && let Some(view) = upper.upgrade()
                                && (view.read(cx).marquee.is_some()
                                    || view.read(cx).pressed_id.is_some())
                            {
                                view.update(cx, |this, cx| {
                                    this.pressed_id = None;
                                    this.finish_marquee(cx);
                                    cx.notify();
                                });
                            }
                        });
                        marker.borrow_mut().painted();
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size(px(1.)),
            )
    }
}
fn number(value: usize) -> String {
    let text = value.to_string();
    let mut out = String::new();
    for (i, c) in text.chars().enumerate() {
        if i > 0 && (text.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
fn configure_native_corners(window: &Window, dark: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DwmSetWindowAttribute,
        },
    };
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        // DWM keeps resizing, snapping and restored/maximized corner behavior in the OS.
        unsafe {
            let immersive = dark as i32;
            let _ = DwmSetWindowAttribute(
                HWND(handle.hwnd.get() as *mut _),
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &immersive as *const _ as *const _,
                std::mem::size_of_val(&immersive) as u32,
            );
            let _ = DwmSetWindowAttribute(
                HWND(handle.hwnd.get() as *mut _),
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const _ as *const _,
                std::mem::size_of_val(&DWMWCP_ROUND) as u32,
            );
        }
    }
}
fn main() {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() == 3 && arguments[1] == "--session-status-file" {
        let result = session::locked();
        let value = serde_json::json!({"locked":result.as_ref().copied().unwrap_or(true),"error":result.err().map(|e|e.to_string())});
        std::fs::write(&arguments[2], serde_json::to_vec(&value).unwrap())
            .expect("session status file");
        return;
    }
    let options = Options::parse();
    Application::with_platform(Rc::new(
        gpui_windows::WindowsPlatform::new(false).expect("Windows graphics initialization"),
    ))
    .with_assets(paint::Assets)
    .run(move |cx| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(options.width), px(options.height)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("拾径 · Backleaf".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_min_size: Some(size(px(560.), px(440.))),
                ..Default::default()
            },
            move |window, cx| cx.new(|cx| Backleaf::new(options, window, cx)),
        )
        .expect("Backleaf window");
        cx.activate(true);
    });
}
