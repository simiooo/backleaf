#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app_archive;
mod app_drag;
mod app_elevation;
mod app_navigation;
mod app_ole;
mod app_operations;
mod app_quick_drag;
mod app_restore;
mod app_scroll;
mod app_search;
mod app_views;
mod app_watch;
mod capacity_input;
mod content_search;
mod directory_snapshot;
mod drag;
mod elevation;
mod file_geometry;
mod file_icons;
mod file_view;
#[cfg(test)]
mod headless_tests;
mod i18n;
mod idle;
mod image_preview;
mod index;
mod index_bulk;
mod index_watch;
mod input;
mod jump_list;
mod layout;
mod locations;
#[cfg(test)]
mod maintenance_benchmark;
mod marquee_scroll;
mod marquee_smoke;
mod menu;
mod metrics;
mod model;
mod native_icons;
mod navigation;
mod ole;
mod operations;
mod paint;
mod popup_menu;
mod properties;
mod properties_capacity;
mod quick_access;
mod scrollbar;
mod search;
#[cfg(test)]
mod search_benchmark;
mod search_cache;
mod search_motion;
mod selectable;
mod selection;
mod selection_bytes;
mod selection_smoke;
mod session;
mod shell_menu;
mod startup;
mod task_panel;
#[cfg(test)]
mod test_support;
mod theme;
mod typeahead;
mod volume_capacity;
mod watcher;
mod window_state;

use gpui::{prelude::*, *};
use metrics::{Metrics, Options};
use model::{Detail, Event, PageCache, Selection, Source};
use paint::{
    FileState, FlowCache, GridTextCache, SharedFlowCache, SharedGridTextCache, file_background,
    grid_name, icon, tooltip,
};
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
    application_icons: native_icons::Cache,
    image_previews: image_preview::Cache,
    search: app_search::SearchUi,
    navigation: app_navigation::NavigationUi,
    files: app_operations::FileOperations,
    drag: app_drag::DragUi,
    ole: Option<ole::Site>,
    ole_target: Option<usize>,
    views: app_views::ViewsUi,
    archive: app_archive::ArchiveUi,
    restore: app_restore::RestoreUi,
    elevation: app_elevation::ElevationUi,
    index: Option<index::Service>,
    index_status: index::Status,
    native_owner_handle: usize,
    colors: &'static Palette,
    session_locked: bool,
    _session: Option<session::SessionMonitor>,
    grid: Entity<FileGrid>,
    options: Options,
    source: Source,
    watch: app_watch::WatchUi,
    path: PathBuf,
    history: Vec<PathBuf>,
    generation: u64,
    next_generation: u64,
    tx: async_channel::Sender<Event>,
    cache: PageCache,
    labels: SharedGridTextCache,
    requested: Range<usize>,
    serial: u64,
    selection: Selection,
    stats: selection_bytes::Stats,
    selection_worker: SelectionWorker,
    marquee: Option<Marquee>,
    keyboard_focus: bool,
    typeahead: typeahead::State,
    hovered_id: Option<usize>,
    pressed_id: Option<usize>,
    suppress_click: bool,
    cell_width: f32,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    scrollbar_drag: Option<app_scroll::Drag>,
    scrollbar_hover: Option<app_scroll::Kind>,
    columns: usize,
    origin_row: usize,
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
    auto_scroll: marquee_scroll::Controller,
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
    fn new(mut options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::startup::mark("new_enter");
        let native_owner = Self::native_owner(window);
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
        let mut navigation = app_navigation::NavigationUi::new(
            navigation::Worker::new(tx.clone()),
            tx.clone(),
            options.fixture.is_none() && native_owner != 0,
            options.state_dir.clone(),
            cx,
        );
        navigation.window_active = window.is_window_active() && window.is_visible();
        crate::idle::set_window_foreground(navigation.window_active);
        if let Some(worker) = &navigation.volume_worker {
            worker.set_active(navigation.window_active);
        }
        let search = app_search::SearchUi::new(search::Worker::with_state_dir(
            tx.clone(),
            options.state_dir.clone(),
        ));
        let application_icons = native_icons::Cache::new(tx.clone());
        let image_previews = image_preview::Cache::new(tx.clone());
        crate::startup::mark("new_workers");
        let (operation_tx, operation_rx) = async_channel::bounded(1);
        let hub = operations::service::Hub::new(options.state_dir.clone(), operation_tx);
        cx.spawn(async move |this, cx| {
            while operation_rx.recv().await.is_ok() {
                if this
                    .update(cx, |this, cx| this.operations_changed(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let close_owner = owner.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            close_owner
                .update(cx, |this, cx| {
                    this.request_close(window, cx);
                    false
                })
                .unwrap_or(true)
        });
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
        // A TestPlatform window has no HWND or desktop session. Its production
        // view and workers still run; only the native OS boundary is absent.
        let session = (native_owner != 0).then(|| session::monitor(tx.clone()));
        let path = options.path.clone().unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:\\"))
        });
        let restored = options.restored.take();
        let state_enabled = options.fixture.is_none() && options.output.is_none();
        let window_id = restored.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| {
            format!(
                "{}-{}",
                window_state::instance(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        });
        let restore = app_restore::RestoreUi {
            store: state_enabled.then(|| {
                window_state::Store::new(options.state_dir.clone(), window_id, tx.clone())
            }),
            revision: restored.as_ref().map_or(0, |r| r.revision),
            last: restored.as_ref().map(|r| r.state.clone()),
            pending_search: restored.as_ref().and_then(|r| r.state.search.clone()),
            closing: None,
            ready_close: false,
            closed: false,
            close_requested: false,
            restored: restored.is_some(),
            failed: false,
            initial_error: options.restore_error.take(),
        };
        let elevation = app_elevation::ElevationUi::new(options.elevation_restore.take());
        let quit = cx.on_app_quit(|this, cx| {
            let (sender, receiver) = async_channel::bounded(1);
            if let Some(store) = &this.restore.store
                && !this.restore.closed
                && let Some(mut state) = this.restore.last.clone()
            {
                state.path = file_view::WinPath::new(&this.path);
                if this.search.open {
                    let mut definition = this.search.definition.clone();
                    if let Some(input) = &this.search.input
                        && !input.read(cx).is_composing()
                    {
                        definition.query = input.read(cx).value();
                    }
                    state.search = Some(window_state::SearchState {
                        definition,
                        saved: this.search.saved.clone(),
                    });
                } else {
                    state.search = None;
                }
                this.restore.revision += 1;
                store.save(
                    this.restore.revision,
                    state,
                    this.restore.closing.is_some(),
                    Some(sender),
                );
            }
            async move {
                let _ = receiver.recv().await;
            }
        });
        let index = options.fixture.is_none().then(|| {
            index::Service::new(
                options.state_dir.clone(),
                options
                    .index_roots
                    .clone()
                    .or_else(|| options.output.as_ref().map(|_| Vec::new())),
                tx.clone(),
            )
        });
        crate::startup::mark("new_index");
        let drag = app_drag::DragUi::new(drag::Worker::new(tx.clone()));
        let watch = app_watch::WatchUi {
            generation: 1,
            monitor: options
                .fixture
                .is_none()
                .then(|| watcher::Monitor::new(path.clone(), 1, tx.clone())),
            applied: 1,
            ..Default::default()
        };
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
        let visibility = cx.observe_window_visibility(window, |this, visibility, window, cx| {
            this.navigation.window_active = visibility.is_visible() && window.is_window_active();
            crate::idle::set_window_foreground(this.navigation.window_active);
            if let Some(worker) = &this.navigation.volume_worker {
                worker.set_active(this.navigation.window_active && !this.session_locked);
            }
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
        cx.spawn(async move |this, cx| {
            let mut day = chrono::Local::now().format("%Y-%m-%d%z").to_string();
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let now = chrono::Local::now().format("%Y-%m-%d%z").to_string();
                if this
                    .update(cx, |this, cx| {
                        if now != day
                            && this.search.open
                            && this.search.definition.modified_days.is_some()
                            && this.navigation.window_active
                        {
                            this.refresh_search();
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                day = now;
            }
        })
        .detach();
        let panel =
            options.fixture.is_some() && (options.navigation_fixture == 0 || options.flow > 0);
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
            this.navigation.window_active = window.is_window_active() && window.is_visible();
            crate::idle::set_window_foreground(this.navigation.window_active);
            if let Some(worker) = &this.navigation.volume_worker {
                worker.set_active(this.navigation.window_active && !this.session_locked);
            }
            if !window.is_window_active() {
                this.navigation.menu_focus_transfer =
                    this.files.menu.is_some() || this.navigation.menu.is_some();
                this.reset_typeahead();
                this.cancel_file_drag();
                this.navigation.drag = None;
                if this.marquee.is_some() {
                    this.metrics
                        .borrow_mut()
                        .event("marquee-stop window-deactivated");
                }
                this.scrollbar_drag = None;
                this.navigation.hovered = false;
                this.commit_rename_blur(cx);
                this.finish_marquee(cx);
                this.pressed_id = None;
                this.hovered_id = None;
            }
            if window.is_window_active()
                && !std::mem::take(&mut this.navigation.menu_focus_transfer)
            {
                // Bookkeeping generations change on every committed refresh; only
                // touch the directory when the watcher actually reports a change.
                // Otherwise focus/minimize transitions redraw everything for no
                // filesystem reason.
                if this.watch_stale() {
                    this.mark_directory_dirty(cx);
                }
                this.refresh_search();
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
            cx.notify();
        });
        if options.selection_smoke {
            selection_smoke::schedule(window, cx);
        }
        if options.marquee_smoke {
            marquee_smoke::schedule(window, cx);
        }
        let mut ole_error = None;
        let ole = if options.fixture.is_none() && native_owner != 0 {
            ole::Site::new(
                Self::native_owner(window),
                options.state_dir.join("ole"),
                hub.clone(),
                tx.clone(),
                window.window_handle(),
                cx,
            )
            .map_err(|error| {
                ole_error = Some(error.to_string());
            })
            .ok()
        } else {
            None
        };
        crate::startup::mark("new_ole");
        crate::startup::mark("new_done");
        Self {
            application_icons,
            image_previews,
            ole,
            ole_target: None,
            drag,
            index,
            index_status: index::Status::default(),
            views: app_views::ViewsUi::default(),
            archive: app_archive::ArchiveUi::default(),
            restore,
            elevation,
            search,
            navigation,
            files: app_operations::FileOperations::new(hub),
            native_owner_handle: Self::native_owner(window),
            colors,
            session_locked: native_owner != 0,
            _session: session,
            grid,
            options,
            source,
            watch,
            path,
            history: vec![],
            generation: 1,
            next_generation: 1,
            stats: selection_bytes::Stats::new(tx.clone()),
            tx,
            cache: PageCache::new(),
            labels: Rc::new(RefCell::new(GridTextCache::default())),
            requested: 0..0,
            serial: 0,
            selection: Selection::default(),
            selection_worker,
            marquee: None,
            keyboard_focus: false,
            typeahead: typeahead::State::default(),
            hovered_id: None,
            pressed_id: None,
            suppress_click: false,
            cell_width: layout::LAYOUT.grid.minimum_cell_width,
            focus,
            scroll: UniformListScrollHandle::new(),
            scrollbar_drag: None,
            scrollbar_hover: None,
            columns: 1,
            origin_row: 0,
            detail_token: 0,
            detail_pending: false,
            open_pending: false,
            error: ole_error,
            panel,
            task,
            flow_cache,
            clock_pending: false,
            benchmark_step: 0,
            last_motion_frame: 0,
            metrics,
            _subscriptions: vec![visibility, closed, activated, appearance, quit],
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
            report.application_icons = self.application_icons.stats();
            if let Ok(json) = serde_json::to_vec_pretty(&report) {
                let _ = std::fs::write(path, json);
            }
        }
    }
    fn receive(&mut self, event: Event, cx: &mut Context<Self>) {
        if self.receive_operation(&event, cx) {
            return;
        }
        match event {
            Event::SelectionBytes {
                generation,
                token,
                result,
            } => {
                self.stats.accept(generation, token, result);
                cx.notify();
            }
            Event::Count {
                generation,
                count,
                done,
                error,
                denied,
            } if self
                .watch
                .pending
                .as_ref()
                .is_some_and(|p| p.generation == generation) =>
            {
                self.refresh_count(count, done, error, denied, cx);
            }
            Event::Refreshed { generation, result }
                if self
                    .watch
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.generation == generation) =>
            {
                match result {
                    Ok(ready) => self.watch.pending.as_mut().unwrap().ready = Some(ready),
                    Err(error) => self.refresh_failed(error, cx),
                }
                cx.notify();
            }
            Event::OleChanged => {
                if let Some(site) = &self.ole {
                    site.changed();
                }
                cx.notify();
            }
            Event::OleTask(id) => {
                self.show_operation(id, cx);
                cx.notify();
            }
            Event::OlePin {
                path,
                before,
                revision,
            } => {
                let title = navigation::display_name(&path);
                if let Err(error) = self.navigation.store.send(quick_access::Action::Pin {
                    path,
                    title,
                    before,
                    revision,
                }) {
                    self.navigation_error(error.to_string(), cx);
                }
            }
            Event::OleSourceEnded(result) => {
                if result < 0 {
                    self.navigation_error(crate::i18n::tr_owned("拖放未完成"), cx);
                }
                if result == 0x00040100 {
                    self.mark_directory_dirty(cx);
                }
            }
            Event::ShellRows(notice) => {
                if let Some(menu) = self.files.menu.clone() {
                    menu.update(cx, |m, cx| m.shell_notice(notice, cx));
                }
            }
            Event::ShellMenuClosed(result) => {
                if let Err(error) = result {
                    self.navigation_error(error, cx);
                }
                self.mark_directory_dirty(cx);
                self.refresh_search();
            }
            Event::IndexError(error) => self.navigation_error(error, cx),
            Event::IndexStatus(status) => {
                let changed = status.revision != self.index_status.revision;
                self.index_status = status;
                if changed
                    && self.search.open
                    && self.search.worker.used_index()
                    && self.search.started
                {
                    self.refresh_search();
                }
                cx.notify();
            }
            Event::WindowStateSaved {
                revision,
                closed,
                result,
            } => self.save_state_result(revision, closed, result, cx),
            Event::ElevationLaunched(result) => self.accept_elevation_launch(result, cx),
            Event::ElevationSelection {
                generation,
                ids,
                scroll_row,
            } => self.accept_elevation_selection(generation, ids, scroll_row, cx),
            Event::ArchivePrepared { token, result } => self.accept_archive(token, result, cx),
            Event::QuickAccess(notice) => {
                let revision_changed = matches!(notice, crate::quick_access::Notice::Loaded { .. });
                self.accept_quick_access(notice, cx);
                if revision_changed {
                    jump_list::request_update(self.options.state_dir.clone());
                }
            }
            Event::DragTarget {
                generation,
                token,
                result,
            } if generation == self.generation && token == self.drag.token => {
                if let Some(g) = self.drag.gesture.as_mut() {
                    g.target = Some(result);
                }
                cx.notify();
            }
            Event::DirectoryDirty { generation, epoch } if generation == self.watch.generation => {
                if epoch != self.watch.applied {
                    self.mark_directory_dirty(cx);
                }
            }
            Event::PrefixLocated {
                generation,
                token,
                id,
            } if generation == self.generation
                && token == self.typeahead.token
                && self.typeahead.pending
                && self.files.editor.is_none()
                && self.archive.draft.is_none()
                && !self.view_editing()
                && !self.files.close_guard
                && self.files.menu.is_none()
                && !self.navigation.open
                && !self.search.open =>
            {
                self.typeahead.pending = false;
                if let Some(id) = id.filter(|id| !self.files.removed.contains(*id as u64)) {
                    self.selection.select(id, false, false);
                    self.keyboard_focus = true;
                    self.files.focus_grid = true;
                    self.scroll_global(id / self.columns, ScrollStrategy::Nearest, false);
                }
                cx.notify();
            }
            Event::Search(notice) => self.accept_search(notice, cx),
            Event::ContentSearch(notice) => self.accept_content_search(notice, cx),
            Event::Navigation(notice) => self.accept_navigation(notice, cx),
            Event::ApplicationIconsChanged => cx.notify(),
            Event::ImagePreviewsChanged => cx.notify(),
            Event::Session { locked } => {
                if let Some(worker) = &self.navigation.volume_worker {
                    worker.set_active(!locked && self.navigation.window_active);
                }
                if locked {
                    self.finish_marquee(cx);
                }
                self.session_locked = locked;
                if locked {
                    self.scrollbar_drag = None;
                }
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
                denied,
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
                    if denied && !self.elevation.elevated {
                        // The list cannot be read with this token. Keep the
                        // failure visible via the administrator handoff prompt.
                        self.restore.failed = false;
                        self.elevation_prompt(self.path.clone(), cx);
                    } else {
                        // An elevated token already ran into an explicit deny:
                        // no further UAC can help, show the real error.
                        self.restore.failed = self.restore.restored;
                        self.error = error;
                    }
                } else if done {
                    self.restore.failed = false;
                    self.elevation_begin_restore();
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
        let Some(detail) = self.accept_operation_detail(detail, cx) else {
            return;
        };
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
            if let Err(error) =
                properties::open(detail, self.colors, self.options.reduced_motion, cx)
            {
                self.error = Some(crate::trf!("无法打开详情：{}", error));
            }
            cx.notify();
        }
    }
    fn navigate(&mut self, path: PathBuf, push: bool, cx: &mut Context<Self>) {
        if self.files.close_guard {
            return;
        }
        if self.defer_navigation(path.clone(), push, cx) {
            return;
        }
        self.reset_typeahead();
        self.cancel_file_drag();
        self.navigation.drag = None;
        self.cancel_marquee();
        self.scrollbar_drag = None;
        self.close_search();
        if self.options.fixture.is_some() {
            return;
        }
        if push {
            self.history.push(self.path.clone());
        }
        self.next_generation += 1;
        self.generation = self.next_generation;
        self.files.editor = None;
        self.close_path_tree();
        self.navigation.rows.clear();
        self.navigation.token += 1;
        self.files.menu = None;
        self.navigation.menu = None;
        self.navigation.pending = None;
        self.files.removed.clear();
        self.detail_token += 1;
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
        self.watch = app_watch::WatchUi {
            generation: self.generation,
            monitor: Some(watcher::Monitor::new(
                self.path.clone(),
                self.generation,
                self.tx.clone(),
            )),
            applied: 1,
            ..Default::default()
        };
        self.source = Source::directory(self.path.clone(), self.generation, self.tx.clone());
        cx.notify();
    }
    fn request_detail(&mut self, id: usize, open: bool, cx: &mut Context<Self>) {
        if self.watch_stale() {
            self.mark_directory_dirty(cx);
            return;
        }
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
            let cut = self
                .files
                .hub
                .own_clipboard()
                .filter(|c| c.cut)
                .map(|c| c.ledger.as_ref().clone());
            self.source.request_with_cut(self.serial, range, cut);
        }
        let mut m = self.metrics.borrow_mut();
        m.cache = m.cache.max(self.cache.len());
    }
    fn grid_width(&self, window: &Window) -> f32 {
        f32::from(window.viewport_size().width) - self.sidebar_width()
    }
    fn global_point(&self, p: Point<Pixels>) -> (f64, f64) {
        let offset = -f32::from(self.scroll.0.borrow().base_handle.offset().y) as f64;
        (
            f64::from(f32::from(p.x) - layout::content_left(self.sidebar_width())),
            self.origin_row as f64 * layout::row_step() as f64 + offset + f32::from(p.y) as f64
                - f64::from(layout::content_top()),
        )
    }
    fn hit_file(&self, p: Point<Pixels>, window: &Window) -> Option<usize> {
        let (x, y) = self.global_point(p);
        if x < 0.
            || y < 0.
            || f32::from(p.x)
                >= self.sidebar_width() + self.grid_width(window)
                    - layout::SURFACE.margin_x
                    - layout::SURFACE.padding_x
                    - layout::SCROLL.gutter
            || f32::from(p.y) < layout::content_top()
            || f32::from(p.y) > layout::content_bottom(f32::from(window.viewport_size().height))
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
        if id >= self.source.count() || self.files.removed.contains(id as u64) {
            return None;
        }
        let dx = x - col as f64 * (self.cell_width as f64 + layout::LAYOUT.grid.column_gap as f64);
        let dy = y - row as f64 * layout::row_step() as f64;
        if !self.cache.contains_key(&id) {
            return None;
        }
        let (name_width, name_height) = self.labels.borrow().extents.get(&id).copied()?;
        crate::file_geometry::surface(self.cell_width, name_width, name_height)
            .rounded_contains(dx as f32, dy as f32, layout::LAYOUT.file.radius)
            .then_some(id)
    }
    fn selection_busy(&self) -> bool {
        self.marquee
            .as_ref()
            .is_some_and(|m| m.active && (m.held || m.accepted != m.token))
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
    fn reset_typeahead(&mut self) {
        self.typeahead.reset();
        self.source.prefix(self.typeahead.token, None, 0);
    }
    fn typeahead_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = event.keystroke.modifiers;
        if !window.is_window_active()
            || !window.is_visible()
            || self.files.editor.is_some()
            || self.archive.draft.is_some()
            || self.view_editing()
            || self.files.close_guard
            || self.search.open
            || self.navigation.open
            || self.files.menu.is_some()
            || self.selection_busy()
            || self.drag.gesture.is_some()
            || self.navigation.drag.is_some()
            || m.control
            || m.alt
            || m.platform
            || event.prefer_character_input
            || event.is_held
            || typeahead::native_composing(self.native_owner_handle)
        {
            return false;
        }
        let Some(text) = event.keystroke.key_char.as_deref().or_else(|| {
            (event.keystroke.key.chars().count() == 1).then_some(event.keystroke.key.as_str())
        }) else {
            return false;
        };
        if text == " " && !self.typeahead.continuing(Instant::now()) {
            return false;
        }
        if let Some((token, prefix)) = self.typeahead.push(text, Instant::now()) {
            self.source
                .prefix(token, Some(prefix), self.typeahead.cycle);
            cx.notify();
            return true;
        }
        false
    }
    fn begin_marquee(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_typeahead();
        if self.rename_editing() {
            if self.hit_file(event.position, window).is_none() {
                self.selection = Selection::default();
                self.commit_rename_selection(cx);
            }
            return;
        }
        if self.selection_busy() {
            return;
        }
        self.cancel_marquee();
        self.suppress_click = false;
        self.keyboard_focus = false;
        if let Some(id) = self.hit_file(event.position, window) {
            self.pressed_id = Some(id);
            self.begin_file_drag(id, event);
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
            auto_scroll: marquee_scroll::Controller::default(),
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
            m.pointer.x.clamp(
                px(layout::content_left(self.sidebar_width())),
                px(self.sidebar_width() + self.grid_width(window)
                    - layout::SURFACE.margin_x
                    - layout::SURFACE.padding_x
                    - layout::SCROLL.gutter),
            ),
            m.pointer.y.clamp(
                px(layout::content_top()),
                px(layout::content_bottom(f32::from(
                    window.viewport_size().height,
                ))),
            ),
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
            self.metrics
                .borrow_mut()
                .event("marquee-stop pointer-without-held-button");
            self.update_marquee_band(window, cx);
            self.finish_marquee(cx);
            return;
        }
        m.pointer = event.position;
        let delta = event.position - m.start;
        if f32::from(delta.x).hypot(f32::from(delta.y)) >= 4. {
            m.active = true;
        }
        let edge = self.marquee_edge(window);
        let m = self.marquee.as_mut().unwrap();
        if m.auto_scroll.direction() != edge.direction || edge.direction == 0 {
            m.last_tick = Instant::now();
        }
        m.auto_scroll.observe(edge);
        self.update_marquee_band(window, cx);
    }
    fn finish_marquee(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.marquee.as_mut() else {
            return;
        };
        m.held = false;
        m.auto_scroll.reset();
        if m.active {
            self.suppress_click = true;
        }
        if !m.active || m.accepted == m.token {
            self.marquee = None;
        }
        cx.notify();
    }
    fn marquee_edge(&self, window: &Window) -> marquee_scroll::Edge {
        let Some(m) = &self.marquee else {
            return marquee_scroll::Edge::default();
        };
        if !m.active
            || !m.held
            || self.session_locked
            || !window.is_window_active()
            || !window.is_visible()
            || !self.focus.is_focused(window)
        {
            return marquee_scroll::Edge::default();
        }
        let Some(g) = self.scroll_geometry(app_scroll::Kind::Files, window) else {
            return marquee_scroll::Edge::default();
        };
        marquee_scroll::edge_at(
            f64::from(f32::from(m.pointer.y)),
            f64::from(layout::content_top()),
            f64::from(layout::content_bottom(f32::from(
                window.viewport_size().height,
            ))),
            g.offset,
            g.maximum,
        )
    }
    fn marquee_velocity(&self, window: &Window) -> f64 {
        let Some(m) = &self.marquee else {
            return 0.;
        };
        let Some(g) = self.scroll_geometry(app_scroll::Kind::Files, window) else {
            return 0.;
        };
        m.auto_scroll
            .velocity(self.marquee_edge(window), g.viewport, g.maximum, g.offset)
    }
    fn step_marquee(&mut self, window: &Window, cx: &mut Context<Self>) {
        let edge = self.marquee_edge(window);
        if edge.direction == 0 {
            if let Some(m) = &mut self.marquee {
                m.auto_scroll.reset();
                m.last_tick = Instant::now();
            }
            return;
        }
        let Some(g) = self.scroll_geometry(app_scroll::Kind::Files, window) else {
            return;
        };
        let m = self.marquee.as_mut().unwrap();
        let now = Instant::now();
        let delta = now.duration_since(m.last_tick).as_secs_f64();
        m.last_tick = now;
        let next = m
            .auto_scroll
            .advance(edge, delta, g.viewport, g.maximum, g.offset);
        self.set_scroll_offset(app_scroll::Kind::Files, next, window, cx);
        self.update_marquee_band(window, cx);
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.control
            && event.keystroke.modifiers.shift
            && event.keystroke.key == "l"
        {
            crate::i18n::toggle();
            crate::i18n::save(&self.options.state_dir);
            jump_list::request_update(self.options.state_dir.clone());
            cx.notify();
            return;
        }
        if self.ole.as_ref().is_some_and(|site| site.busy()) {
            cx.stop_propagation();
            return;
        }
        if self.elevation_key(event, window, cx) {
            return;
        }
        if self.restore.closing.is_some() {
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && event.keystroke.key == "n"
            && self.options.fixture.is_none()
        {
            if !event.is_held {
                let mut options = self.options.clone();
                options.output = None;
                options.restored = None;
                options.path = Some(self.path.clone());
                options.width = f32::from(window.viewport_size().width);
                options.height = f32::from(window.viewport_size().height);
                let mut state = self.capture_window_state(window, cx);
                state.bounds[0] += 24.;
                state.bounds[1] += 24.;
                state.maximized = false;
                options.restored = Some(window_state::Restored {
                    id: format!(
                        "{}-{}",
                        window_state::instance(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    ),
                    revision: 0,
                    state,
                });
                open_backleaf_window(options, cx);
            }
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.control && event.keystroke.key == "q" {
            self.request_close(window, cx);
            cx.stop_propagation();
            return;
        }

        if event.keystroke.key == "escape" && self.navigation.drag.is_some() {
            self.navigation.drag = None;
            self.navigation.suppress_click = true;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.keystroke.key == "escape" && self.drag.gesture.is_some() {
            self.cancel_file_drag();
            self.pressed_id = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.search_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.search.open {
            return;
        }
        if self.navigation_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.typeahead_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.file_shortcut(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.files.editor.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" && self.scrollbar_drag.take().is_some() {
            cx.notify();
            return;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || (key == "escape" && (modifiers.control || modifiers.alt)) {
            return;
        }
        self.metrics.borrow_mut().input("key");
        self.reset_typeahead();
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
            if self.detail_pending {
                self.detail_token += 1;
                self.detail_pending = false;
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
        if modifiers.alt && key != "enter" {
            return;
        }
        if modifiers.control && key == "a" {
            self.selection.all(self.source.count());
            self.selection.mask.exclude(&self.files.removed);
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
            * ((f32::from(window.viewport_size().height) - layout::grid_reserved_height())
                / layout::row_step())
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
        // Removed items stay visible (dimmed) until the next snapshot commits instead
        // of leaving an empty cell that reads as a large blank row.
        let removed = self.files.removed.contains(id as u64);
        let item = self.cache.get(&id).cloned();
        let icon_kind = item.as_ref().map_or(file_icons::FileIcon::Generic, |i| {
            file_icons::FileIcon::for_name(&i.os_name, i.directory)
        });
        let native_image =
            if self.options.fixture.is_none() && icon_kind == file_icons::FileIcon::Application {
                item.as_ref()
                    .and_then(|i| self.application_icons.image(self.path.join(&i.os_name)))
            } else {
                None
            };
        let image_file = item
            .as_ref()
            .is_some_and(|i| !i.directory && crate::image_preview::supported_name(&i.os_name));
        let image_preview = if image_file && self.options.fixture.is_none() {
            item.as_ref()
                .and_then(|i| self.image_previews.image(self.path.join(&i.os_name)))
        } else {
            None
        };
        let outlined =
            native_image.is_none() && icon_kind != file_icons::FileIcon::Folder && !image_file;
        let artwork = native_image.unwrap_or_else(|| icon_kind.asset(self.colors.dark).into());
        let name = item.as_ref().map(|x| x.name.clone()).unwrap_or_default();
        let label = item.as_ref().map(|x| x.label.clone()).unwrap_or_default();
        let active = window.is_window_active();
        let focused = self.selection.focus == Some(id);
        let grid_focused = self.focus.is_focused(window);
        let state = FileState {
            selected: self.is_selected(id),
            active,
            focused: focused && self.keyboard_focus && grid_focused,
            hovered: self.ole_target == Some(id)
                || active
                    && (self.hovered_id == Some(id)
                        || self.drag.gesture.as_ref().is_some_and(|g| {
                            g.active && g.target_id == Some(id) && matches!(g.target, Some(Ok(_)))
                        }))
                    && self.marquee.is_none(),
            pressed: active && self.pressed_id == Some(id),
        };
        let colors = self.colors;
        let l = &layout::LAYOUT;
        // The row keeps a 36 px two-line name slot; an image may use the full
        // remaining height and the cell's inner width without overflowing.
        let preview_width = (width - 2. * l.file.visual_padding_x).max(l.grid.icon_size);
        let preview_height = (l.grid.cell_height
            - l.file.visual_padding_top
            - l.file.visual_padding_bottom
            - l.file.icon_name_gap
            - 36.)
            .max(l.grid.icon_size);
        let art_width = if image_file {
            preview_width
        } else {
            l.grid.icon_size
        };
        let art_height = if image_file {
            preview_height
        } else {
            l.grid.icon_size
        };
        let editor = self
            .files
            .editor
            .as_ref()
            .filter(|e| matches!(&e.kind,app_operations::EditKind::Rename(d)if d.id==id))
            .map(|e| e.input.clone());
        let caption_hit = self
            .labels
            .borrow()
            .extents
            .get(&id)
            .copied()
            .map(|(w, h)| {
                if image_file {
                    crate::file_geometry::caption_with_art(width, w, h, art_height)
                } else {
                    crate::file_geometry::caption(width, w, h)
                }
            });
        div()
            .id(SharedString::from(format!("file-{}-{id}", self.generation)))
            .role(Role::ListBoxOption)
            .aria_label(name.clone())
            .aria_selected(state.selected)
            .when(focused && grid_focused, |d| d.aria_active_descendant())
            .when(removed, |d| d.opacity(0.35))
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
            .on_hover(cx.listener(move |this, hovered, window, cx| {
                let next = if *hovered && this.hit_file(window.mouse_position(), window) == Some(id)
                {
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
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    if this.marquee.is_some()
                        || this.scrollbar_drag.is_some()
                        || this.drag.gesture.as_ref().is_some_and(|g| g.active)
                    {
                        return;
                    }
                    let next = if this.hit_file(event.position, window) == Some(id) {
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
                }),
            )
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
                if !this.rename_editing() {
                    window.focus(&this.focus, cx);
                }
                let m = event.modifiers();
                this.selection.select(id, m.control, m.shift);
                if this.rename_editing() {
                    this.commit_rename_selection(cx);
                    cx.notify();
                    return;
                }
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
                    if !this.rename_editing() {
                        window.focus(&this.focus, cx);
                    }
                    this.open_menu(event.position, Some(id), false, window, cx);
                    cx.stop_propagation();
                }),
            )
            .when(item.is_some(), |d| {
                d.child(file_background(
                    id,
                    label.clone(),
                    state,
                    art_width,
                    art_height,
                    self.labels.clone(),
                    colors,
                ))
            })
            .child({
                let cut = item.as_ref().is_some_and(|i| i.cut);
                if image_file {
                    let graphic = match image_preview {
                        Some(source) => paint::thumbnail(source, preview_width, preview_height),
                        None => div()
                            .w(px(preview_width))
                            .h(px(preview_height))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(paint::file_artwork(
                                artwork,
                                l.grid.icon_size,
                                outlined,
                                colors,
                            )),
                    };
                    graphic.when(cut, |s| s.opacity(0.42))
                } else {
                    paint::file_artwork(artwork, l.grid.icon_size, outlined, colors)
                        .when(cut, |s| s.opacity(0.42))
                        .when(item.is_none(), |s| s.opacity(0.2))
                }
            })
            .child(
                div()
                    .id(("filename-tooltip", id))
                    .relative()
                    .w_full()
                    .h(px(36.))
                    .flex_none()
                    .when(
                        self.marquee.is_none() && item.is_some() && editor.is_none(),
                        |d| {
                            d.when_some(caption_hit, |d, bounds| {
                                d.child(
                                    div()
                                        .id(("filename-tooltip-hit", id))
                                        .absolute()
                                        .top_0()
                                        .left(px((bounds.left - l.file.visual_padding_x).max(0.)))
                                        .w(px(bounds
                                            .width
                                            .min((width - 2. * l.file.visual_padding_x).max(0.))))
                                        .h(px(bounds.height.min(36.)))
                                        .tooltip(tooltip(name.clone(), colors)),
                                )
                            })
                        },
                    )
                    .when_some(editor.clone(), |d, input| {
                        d.flex()
                            .items_center()
                            .justify_center()
                            .child(div().w(px(96.)).max_w_full().child(input))
                    })
                    .when(item.is_some() && editor.is_none(), |d| {
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
        let label = crate::i18n::tr(label);
        let action = Rc::new(click);
        let keyboard = action.clone();
        div()
            .id(id)
            .size(px(layout::CHROME.control_size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(layout::CHROME.control_radius))
            .border_1()
            .border_color(transparent_black())
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .tooltip(tooltip(label.to_owned(), colors))
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(false))
            })
            .active(move |d| d.bg(colors.color(ColorRole::Pressed)))
            .focus(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(true))
            })
            .on_key_down(move |event, window, cx| {
                if paint::activation_key(event) {
                    cx.stop_propagation();
                    keyboard(&ClickEvent::default(), window, cx);
                }
            })
            .on_click(move |event, window, cx| action(event, window, cx))
            .child(icon(id, colors))
    }
    fn titlebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
            .h(px(layout::CHROME.bar_height))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .bg(colors.color(ColorRole::Chrome))
            .px(px(8.))
            .py(px(layout::CHROME.vertical_inset))
            .gap(px(layout::CHROME.control_gap))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(layout::CHROME.control_gap))
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
                    )),
            )
            .child(
                if let Some(editor) = self
                    .files
                    .editor
                    .as_ref()
                    .filter(|e| matches!(e.kind, app_operations::EditKind::Path))
                {
                    div()
                        .w(px(self.path_available(window, cx)))
                        .min_w_0()
                        .max_w(px(self.path_available(window, cx)))
                        .child(editor.input.clone())
                        .into_any_element()
                } else {
                    self.path_control(title, window, cx)
                },
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(layout::CHROME.minimum_drag_width))
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            .when(!self.visible_operations().is_empty(), |d| {
                d.child(self.operation_entry(cx))
            })
            .when(self.options.fixture.is_none(), |d| {
                d.child(self.search_entry(window, cx))
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(layout::CHROME.caption_gap))
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
                                .w(px(layout::CHROME.caption_width))
                                .h(px(layout::CHROME.control_size))
                                .rounded(px(layout::CHROME.control_radius))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .role(Role::Button)
                                .aria_label(label)
                                .tooltip(tooltip(label.to_owned(), colors))
                                .focusable()
                                .tab_stop(true)
                                .focus(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(true))
                                })
                                .on_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, window, cx| {
                                        if paint::activation_key(event) {
                                            cx.stop_propagation();
                                            match area {
                                                WindowControlArea::Min => window.minimize_window(),
                                                WindowControlArea::Max => window.zoom_window(),
                                                WindowControlArea::Close => {
                                                    this.request_close(window, cx)
                                                }
                                                _ => {}
                                            }
                                        }
                                    },
                                ))
                                .window_control_area(area)
                                .hover(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(false))
                                })
                                .on_click(cx.listener(move |this, _, window, cx| match area {
                                    WindowControlArea::Min => window.minimize_window(),
                                    WindowControlArea::Max => window.zoom_window(),
                                    WindowControlArea::Close => this.request_close(window, cx),
                                    _ => {}
                                }))
                                .child(icon(name, colors))
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.dismiss_titlebar_popups(window, cx)),
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
            && ((self.panel && self.task.read(cx).expanded && self.options.flow > 0)
                || self.files.animating()
                || self.search.animating())
            && !self.options.reduced_motion
            && !cx.reduce_motion()
            && window.is_visible();
        let scripted = !self.session_locked
            && self.options.output.is_some()
            && self.options.scroll
            && window.is_visible();
        let selecting =
            !self.session_locked && window.is_visible() && self.marquee_velocity(window) != 0.;
        if self.clock_pending
            || !(animate || scripted || selecting || self.quick_drag_velocity(window) != 0.)
        {
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
        let search_frame = self.search.animating();
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
                    this.step_quick_drag(window, cx);
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
                    if search_frame {
                        cx.notify();
                    }
                    if this.files.animating() && !this.options.reduced_motion && !cx.reduce_motion()
                    {
                        cx.notify();
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
        let grid_width = (width
            - self.sidebar_width()
            - 2. * (layout::SURFACE.margin_x + layout::SURFACE.padding_x)
            - layout::SCROLL.gutter)
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
        let visible_height =
            f32::from(window.viewport_size().height) - layout::grid_reserved_height();
        let scrollbar = self.scrollbar(app_scroll::Kind::Files, window, cx);
        div()
            .id("file-viewport")
            .relative()
            .role(Role::ListBox)
            .aria_label(crate::i18n::tr("文件"))
            .size_full()
            .pl(px(layout::SURFACE.padding_x))
            .pr(px(layout::SURFACE.padding_x + layout::SCROLL.gutter))
            .py(px(layout::SCROLL.inset_y))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_marquee))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.hit_file(event.position, window).is_none() {
                        this.open_menu(event.position, None, false, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
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
                                    .items_baseline()
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
            .children(scrollbar)
            .into_any_element()
    }
}
impl Render for Backleaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_ole(window, cx);
        self.sync_operation_focus(window, cx);
        self.sync_watcher(window, cx);
        self.sync_view_focus(window, cx);
        self.sync_archive_focus(window, cx);
        self.sync_restore(window, cx);
        self.application_icons.sync(
            self.generation,
            !self.session_locked && window.is_visible(),
            window,
        );
        self.image_previews.sync(
            self.watch.applied,
            !self.session_locked && window.is_visible(),
            window,
        );
        let colors = self.colors;
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        self.metrics.borrow_mut().accessibility_active |= window.is_a11y_active();
        self.metrics
            .borrow_mut()
            .begin([width, height], window.scale_factor());
        let count = self.source.count();
        if self.source.finished() && !self.cache.is_empty() {
            crate::startup::ready("first_ready_render");
        }
        self.arm_clock(window, cx);
        if !self.selection_busy() {
            self.stats
                .sync(&self.source, self.generation, &self.selection.mask);
        }
        let status = if let Some(m) = &self.marquee
            && m.active
            && (m.accepted != m.token || m.held)
        {
            if m.additive && m.accepted != m.token {
                crate::i18n::tr_owned("正在选择…")
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
            format!(
                "已选 {} 项{}",
                number(self.selection.count()),
                self.stats
                    .suffix()
                    .map_or(String::new(), |s| format!(" · {s}"))
            )
        } else {
            crate::trf!("{} 项", number(count))
        };
        let marker = self.metrics.clone();
        let owner = cx.entity().downgrade();
        let overlay = self
            .marquee
            .as_ref()
            .filter(|m| m.active && m.held)
            .map(|m| {
                let top = self
                    .global_point(point(px(12.), px(layout::content_top())))
                    .1;
                let a = point(
                    px((m.anchor.0 + f64::from(layout::content_left(self.sidebar_width()))) as f32),
                    px((m.anchor.1 - top + f64::from(layout::content_top())) as f32),
                );
                let left = f32::from(a.x.min(m.pointer.x)).clamp(
                    layout::content_left(self.sidebar_width()),
                    self.sidebar_width() + self.grid_width(window)
                        - layout::SURFACE.margin_x
                        - layout::SURFACE.padding_x
                        - layout::SCROLL.gutter,
                );
                let right = f32::from(a.x.max(m.pointer.x)).clamp(
                    layout::content_left(self.sidebar_width()),
                    self.sidebar_width() + self.grid_width(window)
                        - layout::SURFACE.margin_x
                        - layout::SURFACE.padding_x
                        - layout::SCROLL.gutter,
                );
                let top = f32::from(a.y.min(m.pointer.y))
                    .clamp(layout::content_top(), layout::content_bottom(height));
                let bottom = f32::from(a.y.max(m.pointer.y))
                    .clamp(layout::content_top(), layout::content_bottom(height));
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
            .bg(colors.color(ColorRole::Chrome))
            .font_family("Microsoft YaHei UI")
            .text_size(px(13.))
            .text_color(colors.color(ColorRole::Primary))
            .on_key_down(cx.listener(Self::key))
            .on_scroll(cx.listener(|this, _, window, cx| {
                this.metrics.borrow_mut().input("wheel");
                if let Some(m) = &mut this.marquee {
                    m.auto_scroll.reset();
                    m.last_tick = Instant::now();
                }
                this.hovered_id = None;
                this.update_marquee_band(window, cx);
                this.grid.update(cx, |_, cx| cx.notify());
            }))
            .child(self.titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(self.sidebar_width() > 0., |d| {
                        d.child(self.sidebar(window, cx))
                    })
                    .child(
                        div()
                            .w(px(self.grid_width(window)))
                            .h_full()
                            .flex_none()
                            .px(px(layout::SURFACE.margin_x))
                            .py(px(layout::SURFACE.margin_y))
                            .child(
                                div()
                                    .size_full()
                                    .relative()
                                    .rounded(px(layout::SURFACE.radius))
                                    .bg(colors.color(ColorRole::Files))
                                    .shadow(colors.file_surface_shadows())
                                    .child(
                                        div()
                                            .size_full()
                                            .rounded(px(layout::SURFACE.radius))
                                            .overflow_hidden()
                                            .child(if window.is_a11y_active() {
                                                self.grid.clone().into_any_element()
                                            } else {
                                                self.grid
                                                    .clone()
                                                    .cached(
                                                        StyleRefinement::default()
                                                            .w(px((self.grid_width(window)
                                                                - 2. * layout::SURFACE.margin_x)
                                                                .max(0.)))
                                                            .h(px((height
                                                                - layout::CHROME.bar_height
                                                                - 2. * layout::SURFACE.margin_y)
                                                                .max(0.))),
                                                    )
                                                    .into_any_element()
                                            }),
                                    ),
                            ),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
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
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size(px(1.)),
            )
            .when(
                self.selection.count() > 0 || self.marquee.is_some() || self.error.is_some(),
                |d| {
                    d.child(
                        div()
                            .id("selection-status-float")
                            .absolute()
                            .left(px(layout::content_left(self.sidebar_width())))
                            .bottom(px(layout::SURFACE.margin_y + 12.))
                            .max_w(px((self.grid_width(window) - 24.).max(1.)))
                            .h(px(layout::SCROLL.status_height))
                            .px(px(layout::SCROLL.status_padding_x))
                            .flex()
                            .items_center()
                            .rounded(px(layout::SCROLL.status_radius))
                            .bg(colors.color(ColorRole::Floating))
                            .border_1()
                            .border_color(colors.color(ColorRole::Border))
                            .shadow(colors.control_shadows(false))
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                div()
                                    .id("status-text")
                                    .truncate()
                                    .role(Role::Label)
                                    .aria_value(status.clone())
                                    .tooltip(tooltip(status.clone(), colors))
                                    .text_size(px(layout::SCROLL.status_font_size))
                                    .line_height(px(layout::SCROLL.status_line_height))
                                    .child(status),
                            ),
                    )
                },
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
            .child(self.drag_overlay(window))
            .child(self.ole_overlay(window))
            .child(self.quick_drag_overlay(window))
            .child(self.operation_overlay(window, cx))
            .child(self.navigation_overlay(window))
            .when(self.navigation.open, |d| {
                d.child(self.path_overlay(window, cx))
            })
            .when(self.search.open, |d| {
                d.child(self.search_overlay(window, cx))
            })
            .child(self.view_popovers(cx))
            .child(self.archive_popup(cx))
            .child(self.restore_failure(cx))
            .child(self.elevation_overlay(window, cx))
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _cx| {
                        let downer = owner.clone();
                        window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = downer.upgrade()
                            {
                                view.update(cx, |this, cx| {
                                    this.reset_typeahead();
                                    this.keyboard_focus = false;
                                    cx.notify();
                                });
                            }
                        });
                        // Stable pane bounds, independent of the scrollbar's occluding hitbox.
                        let hoverer = owner.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = hoverer.upgrade()
                            {
                                let state = view.read(cx);
                                let hovered = scrollbar::inside_sidebar(
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    state.sidebar_width(),
                                    layout::CHROME.bar_height,
                                    f32::from(window.viewport_size().height),
                                );
                                if hovered != state.navigation.hovered {
                                    view.update(cx, |this, cx| {
                                        this.navigation.hovered = hovered;
                                        cx.notify();
                                    });
                                }
                            }
                        });
                        let leaver = owner.clone();
                        window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = leaver.upgrade()
                                && view.read(cx).navigation.hovered
                            {
                                view.update(cx, |this, cx| {
                                    this.navigation.hovered = false;
                                    cx.notify();
                                });
                            }
                        });
                        let mover = owner.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && let Some(view) = mover.upgrade()
                                && (view.read(cx).marquee.is_some()
                                    || view.read(cx).scrollbar_drag.is_some()
                                    || view.read(cx).drag.gesture.is_some()
                                    || view.read(cx).navigation.drag.is_some())
                            {
                                view.update(cx, |this, cx| {
                                    if this.navigation.drag.is_some() {
                                        this.move_quick_drag(event, cx);
                                        if this.navigation.drag.as_ref().is_some_and(|g| g.active) {
                                            cx.stop_propagation();
                                        }
                                    } else if this.drag.gesture.is_some() {
                                        this.move_file_drag(event, window, cx);
                                        if this.drag.gesture.as_ref().is_some_and(|g| g.active) {
                                            cx.stop_propagation();
                                        }
                                    } else if this.scrollbar_drag.is_some() {
                                        this.drag_scrollbar(event, window, cx);
                                        cx.stop_propagation();
                                    } else {
                                        this.move_marquee(event, window, cx);
                                    }
                                });
                            }
                        });
                        let upper = owner.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture
                                && event.button == MouseButton::Left
                                && let Some(view) = upper.upgrade()
                                && (view.read(cx).marquee.is_some()
                                    || view.read(cx).pressed_id.is_some()
                                    || view.read(cx).scrollbar_drag.is_some()
                                    || view.read(cx).drag.gesture.is_some()
                                    || view.read(cx).navigation.drag.is_some())
                            {
                                view.update(cx, |this, cx| {
                                    this.finish_quick_drag(event, cx);
                                    this.finish_file_drag(event, window, cx);
                                    this.scrollbar_drag = None;
                                    this.pressed_id = None;
                                    if this.marquee.is_some() {
                                        this.metrics.borrow_mut().event("marquee-stop mouse-up");
                                    }
                                    this.update_marquee_band(window, cx);
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
fn open_backleaf_window(options: Options, cx: &mut App) {
    crate::startup::mark("window_open");
    let mut bounds = Bounds::centered(None, size(px(options.width), px(options.height)), cx);
    if let Some(saved) = &options.restored {
        let r = saved.state.bounds;
        let proposed = Bounds::new(
            point(px(r[0]), px(r[1])),
            size(px(r[2].max(560.)), px(r[3].max(440.))),
        );
        let fits = cx.displays().iter().any(|d| {
            let b = d.bounds();
            r[0] + 64. > f32::from(b.left())
                && r[0] < f32::from(b.right()) - 64.
                && r[1] + 32. > f32::from(b.top())
                && r[1] < f32::from(b.bottom()) - 32.
        });
        if fits {
            bounds = proposed;
        }
    }
    let window_bounds = if options.restored.as_ref().is_some_and(|r| r.state.maximized) {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
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
}
fn main() {
    crate::startup::arm();
    jump_list::set_app_id();
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() == 3 && arguments[1] == "--session-status-file" {
        let result = session::locked();
        let value = serde_json::json!({"locked":result.as_ref().copied().unwrap_or(true),"error":result.err().map(|e|e.to_string())});
        std::fs::write(&arguments[2], serde_json::to_vec(&value).unwrap())
            .expect("session status file");
        return;
    }
    jump_list::prepare();
    let mut options = Options::parse();
    crate::startup::mark("options");
    i18n::init_with_dir(&options.state_dir);
    jump_list::request_update(options.state_dir.clone());
    let mut startup = Vec::new();
    if let Some(request_path) = options.elevate_request.take() {
        // Started through UAC by another Backleaf window: open the same
        // location and window intent, retrying the access that was denied.
        let parsed = elevation::read_request(&request_path);
        let _ = std::fs::remove_file(&request_path);
        match parsed {
            Ok(request) => {
                let restore = request.restore();
                let mut current = options.clone();
                current.path = Some(request.state.path.path());
                current.width = request.state.bounds[2].max(560.);
                current.height = request.state.bounds[3].max(440.);
                current.elevation_restore = Some(restore);
                current.restored = Some(window_state::Restored {
                    id: format!(
                        "{}-{}",
                        window_state::instance(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    ),
                    revision: 0,
                    state: request.state,
                });
                startup.push(current);
            }
            Err(error) => {
                let mut current = options.clone();
                current.restore_error = Some(error);
                startup.push(current);
            }
        }
    } else if let Some(view) = options.view.clone() {
        // Jump-list saved view: reopen the sidebar view as a fresh search window.
        let mut current = options.clone();
        match jump_list::saved_view_state(&current.state_dir, &view, current.width, current.height)
        {
            Ok(state) => {
                current.path = Some(state.path.path());
                current.restored = Some(window_state::Restored {
                    id: format!(
                        "{}-{}",
                        window_state::instance(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    ),
                    revision: 0,
                    state,
                });
                startup.push(current);
            }
            Err(error) => {
                current.restore_error = Some(error);
                startup.push(current);
            }
        }
    } else if options.new_window {
        // Jump-list task or explicit CLI: bypass window restore and open one
        // fresh window at the home folder.
        startup.push(options.clone());
    } else if options.fixture.is_none() && options.path.is_none() && options.output.is_none() {
        match window_state::load(&options.state_dir) {
            Ok(saved) => {
                for state in saved {
                    let mut current = options.clone();
                    current.path = Some(state.state.path.path());
                    current.width = state.state.bounds[2].max(560.);
                    current.height = state.state.bounds[3].max(440.);
                    current.restored = Some(state);
                    startup.push(current);
                }
            }
            Err(_) => {
                let mut current = options.clone();
                current.restore_error = Some(crate::i18n::tr_owned("无法读取上次窗口状态"));
                startup.push(current);
            }
        }
    }
    if startup.is_empty() {
        startup.push(options);
    }
    crate::startup::mark("window_state");
    let platform =
        gpui_windows::WindowsPlatform::new(false).expect("Windows graphics initialization");
    crate::startup::mark("platform_created");
    Application::with_platform(Rc::new(platform))
        .with_assets(paint::Assets)
        .run(move |cx| {
            crate::startup::mark("platform_run");
            input::bind_keys(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            for options in startup {
                open_backleaf_window(options, cx);
            }
            cx.activate(true);
        });
}
