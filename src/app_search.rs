use super::*;
use crate::{
    input::{InputEvent, TextInput},
    search::{Notice, Row, Worker},
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub struct SearchUi {
    pub(super) refresh: Option<Notice>,
    pub(super) refreshing: bool,
    pub(super) refresh_again: bool,
    pub(super) worker: Worker,
    pub definition: crate::file_view::FileView,
    pub saved: Option<crate::quick_access::Entry>,
    pub open: bool,
    pub(super) motion: crate::search_motion::SearchMotion,
    pub(super) closing_text: String,
    pub(super) input: Option<Entity<TextInput>>,
    pub(super) subscription: Option<Subscription>,
    pub(super) token: u64,
    pub(super) serial: u64,
    pub(super) requested: Range<usize>,
    pub(super) rows: BTreeMap<usize, Row>,
    pub(super) count: usize,
    pub(super) scanned: usize,
    pub(super) unavailable: usize,
    pub(super) skipped: usize,
    pub(super) done: bool,
    pub(super) started: bool,
    pub(super) ever_typed: bool,
    pub(super) query: String,
    pub(super) root: PathBuf,
    pub(super) error: Option<String>,
    pub(super) selected: Option<usize>,
    pub(super) scroll: UniformListScrollHandle,
    pub columns: usize,
    pub content_mode: bool,
    pub content_results: Vec<crate::content_search::Found>,
    pub content_scanned: usize,
    pub content_done: bool,
    pub content_truncated: bool,
    pub content_error: Option<String>,
    pub content_token: u64,
    pub content_latest: Arc<AtomicU64>,
    pub content_selected: Option<usize>,
    pub content_scroll: UniformListScrollHandle,
}
impl SearchUi {
    pub fn new(worker: Worker) -> Self {
        Self {
            refresh: None,
            refreshing: false,
            refresh_again: false,
            worker,
            definition: crate::file_view::FileView::new(std::path::Path::new(".")),
            saved: None,
            open: false,
            motion: crate::search_motion::SearchMotion::default(),
            closing_text: String::new(),
            input: None,
            subscription: None,
            token: 0,
            serial: 0,
            requested: 0..0,
            rows: BTreeMap::new(),
            count: 0,
            scanned: 0,
            unavailable: 0,
            skipped: 0,
            done: false,
            started: false,
            ever_typed: false,
            query: String::new(),
            root: PathBuf::new(),
            error: None,
            selected: None,
            scroll: UniformListScrollHandle::new(),
            columns: 1,
            content_mode: false,
            content_results: Vec::new(),
            content_scanned: 0,
            content_done: false,
            content_truncated: false,
            content_error: None,
            content_token: 0,
            content_latest: Arc::new(AtomicU64::new(0)),
            content_selected: None,
            content_scroll: UniformListScrollHandle::new(),
        }
    }
}
impl SearchUi {
    pub fn animating(&self) -> bool {
        self.motion.animating(Instant::now())
    }
    fn progress(&self, reduced: bool) -> f32 {
        self.motion.value(Instant::now(), reduced)
    }
}
impl Backleaf {
    /// The search popover (results/toolbar/scope) only exists once the inline field
    /// contains a query. Opening search with an empty field keeps just the focused
    /// input and a click-catcher; clearing a previously typed query closes search.
    pub fn search_popover_visible(&self, cx: &App) -> bool {
        self.search.open
            && (!self.search.query.trim().is_empty()
                || self
                    .search
                    .input
                    .as_ref()
                    .is_some_and(|input| !input.read(cx).value().trim().is_empty()))
    }
    pub fn refresh_search(&mut self) {
        if !self.search.open || self.search.query.trim().is_empty() {
            return;
        }
        if self.search.content_mode {
            return;
        }
        if !self.search.started {
            return;
        }
        if self.search.refreshing || !self.search.done {
            self.search.refresh_again = true;
            return;
        }
        self.search.token += 1;
        self.search.refreshing = true;
        self.search.refresh = None;
        self.search.error = None;
        self.search
            .worker
            .search_view(self.search.token, self.search.definition.clone());
    }
    pub fn close_search(&mut self) {
        self.search.refreshing = false;
        self.search.refresh = None;
        self.search.refresh_again = false;
        self.search.open = false;
        self.search.motion.retarget(
            false,
            Instant::now(),
            Duration::from_millis(layout::CHROME.search_close_duration_ms),
        );
        self.search.token += 1;
        self.search.worker.cancel(self.search.token);
        self.search.content_token += 1;
        self.search
            .content_latest
            .store(self.search.content_token, Ordering::Release);
        self.search.content_mode = false;
        self.search.content_results.clear();
        self.search.content_done = false;
        self.search.content_selected = None;
        self.search.content_error = None;
        self.search.content_scanned = 0;
        self.search.content_truncated = false;
        self.search.input = None;
        self.search.subscription = None;
        self.search.rows.clear();
        self.clear_view_filters();
    }
    /// Pressing the title bar blurs the open search popover or path tree, the same
    /// way a click below the bar or Escape already does. Controls that own their
    /// press behavior (the search field and the path dropdown toggle) stop
    /// propagation first, so pressing them keeps their existing behavior.
    pub fn dismiss_titlebar_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.open {
            self.close_search_checked(window, cx);
        }
        if self.navigation.open {
            self.close_path_tree();
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.close_guard || self.options.fixture.is_some() {
            return;
        }
        if self.defer_search(cx) {
            return;
        }
        self.reset_typeahead();
        self.close_path_tree();
        self.cancel_marquee();
        self.files.menu = None;
        self.files.open = false;
        self.panel = false;
        if let Some(input) = &self.search.input {
            window.focus(&input.focus_handle(cx), cx);
            input.update(cx, |i, cx| i.select_full(cx));
            return;
        }
        self.search.open = true;
        self.search.motion.retarget(
            true,
            Instant::now(),
            Duration::from_millis(layout::CHROME.search_duration_ms),
        );
        self.search.closing_text.clear();
        self.files.focus_grid = false;
        self.search.root = self.path.clone();
        self.search.definition = crate::file_view::FileView::new(&self.path);
        self.search.saved = None;
        self.search.query.clear();
        self.search.started = false;
        self.search.ever_typed = false;
        self.search.count = 0;
        self.search.error = None;
        self.search.selected = None;
        self.search.content_mode = false;
        self.search.content_results.clear();
        self.search.content_done = false;
        self.search.content_selected = None;
        self.search.content_error = None;
        self.search.content_scanned = 0;
        self.search.content_truncated = false;
        self.search.content_scroll = UniformListScrollHandle::new();
        self.search.scroll = UniformListScrollHandle::new();
        let input = cx.new(|cx| {
            let mut input = TextInput::new(
                String::new(),
                0..0,
                crate::i18n::tr("搜索文件名"),
                self.colors,
                1024,
                cx,
            );
            input.set_placeholder(crate::i18n::tr("搜索文件名"));
            input.embedded = true;
            input
        });
        self.search.subscription =
            Some(cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                this.search.closing_text = input.read(cx).value();
                match event {
                    InputEvent::Blur(_) => {}
                    InputEvent::Cancel => {
                        this.close_search();
                        this.files.focus_grid = true;
                    }
                    InputEvent::Error(error) => this.search.error = Some(error.clone()),
                    InputEvent::Changed => {
                        this.search.refreshing = false;
                        this.search.refresh = None;
                        this.search.refresh_again = false;
                        this.search.token += 1;
                        this.search.worker.cancel(this.search.token);
                        this.search.rows.clear();
                        this.search.count = 0;
                        this.search.selected = None;
                        this.search.started = false;
                        this.search.error = None;
                        this.search.requested = 0..0;
                        this.search.content_token += 1;
                        this.search
                            .content_latest
                            .store(this.search.content_token, Ordering::Release);
                        this.search.content_results.clear();
                        this.search.content_done = false;
                        this.search.content_selected = None;
                        this.search.content_error = None;
                        this.search.content_scanned = 0;
                        this.search.content_truncated = false;
                        let value = input.read(cx).value();
                        if value.trim().is_empty() {
                            this.search.query.clear();
                            this.search.definition.query.clear();
                            if this.search.ever_typed {
                                // Once a search has actually run, clearing the field
                                // leaves search mode instead of keeping an empty
                                // popover/results surface open.
                                this.close_search();
                                this.files.focus_grid = true;
                            }
                        } else {
                            this.search.ever_typed = true;
                            let token = this.search.token;
                            cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(250))
                                    .await;
                                let _ = this.update(cx, |this, cx| {
                                    if this.search.open
                                        && this.search.token == token
                                        && let Some(input) = &this.search.input
                                        && !input.read(cx).is_composing()
                                    {
                                        this.start_search(input.read(cx).value(), cx);
                                    }
                                });
                            })
                            .detach();
                        }
                    }
                    InputEvent::Submit(value) => {
                        if value.trim().is_empty() {
                            if this.search.ever_typed {
                                this.close_search();
                                this.files.focus_grid = true;
                            }
                        } else if this.search.content_mode {
                            if let Some(index) = this
                                .search
                                .content_selected
                                .or_else(|| (!this.search.content_results.is_empty()).then_some(0))
                            {
                                this.reveal_content_result(index, cx);
                            }
                        } else if this.search.query == value.trim()
                            && this.search.selected.is_some()
                        {
                            this.reveal_search_result(cx);
                        } else {
                            this.start_search(value.clone(), cx);
                        }
                    }
                }
                cx.notify();
            }));
        window.focus(&input.focus_handle(cx), cx);
        self.search.input = Some(input);
        cx.notify();
    }
    pub fn start_search(&mut self, value: String, cx: &mut Context<Self>) {
        self.search.refreshing = false;
        self.search.refresh = None;
        self.search.refresh_again = false;
        let query = value.trim().to_owned();
        if query.is_empty() {
            self.search.query.clear();
            self.search.definition.query.clear();
            self.search.started = false;
            self.search.done = false;
            return;
        }
        if self.search.content_mode {
            self.start_content_search(query, cx);
            return;
        }
        self.search.token += 1;
        self.search.worker.cancel(self.search.token);
        self.search.rows.clear();
        self.search.count = 0;
        self.search.scanned = 0;
        self.search.unavailable = 0;
        self.search.skipped = 0;
        self.search.requested = 0..0;
        self.search.selected = None;
        self.search.error = None;
        self.search.scroll = UniformListScrollHandle::new();
        self.search.query = query.clone();
        self.search.definition.query = query.clone();
        self.search.started = true;
        self.search.done = false;
        self.search
            .worker
            .search_view(self.search.token, self.search.definition.clone());
        cx.notify();
    }
    pub fn start_content_search(&mut self, value: String, cx: &mut Context<Self>) {
        let query = value.trim().to_owned();
        if query.is_empty() {
            self.search.content_results.clear();
            self.search.content_done = false;
            self.search.content_error = None;
            self.search.content_selected = None;
            self.search.started = false;
            self.search.done = false;
            return;
        }
        self.search.content_token += 1;
        let token = self.search.content_token;
        self.search.content_latest.store(token, Ordering::Release);
        self.search.token += 1;
        self.search.worker.cancel(self.search.token);
        self.search.content_results.clear();
        self.search.content_scanned = 0;
        self.search.content_done = false;
        self.search.content_truncated = false;
        self.search.content_error = None;
        self.search.content_selected = None;
        self.search.content_scroll = UniformListScrollHandle::new();
        self.search.query = query.clone();
        self.search.definition.query = query.clone();
        self.search.started = true;
        self.search.done = false;
        self.search.scanned = 0;
        self.search.count = 0;
        self.search.selected = None;
        crate::content_search::spawn(
            token,
            self.search.root.clone(),
            query,
            crate::content_search::DEFAULT_LIMIT,
            self.tx.clone(),
            self.search.content_latest.clone(),
        );
        cx.notify();
    }
    pub fn accept_content_search(
        &mut self,
        notice: crate::content_search::Notice,
        cx: &mut Context<Self>,
    ) {
        let crate::content_search::Notice::Batch {
            token,
            matches,
            scanned,
            done,
            truncated,
            error,
        } = notice;
        if !self.search.open || !self.search.content_mode || token != self.search.content_token {
            return;
        }
        self.search.content_results.extend(matches);
        self.search.content_scanned = scanned;
        self.search.content_done = done;
        self.search.content_truncated |= truncated;
        if let Some(error) = error {
            self.search.content_error = Some(error);
        }
        if self.search.content_selected.is_none() && !self.search.content_results.is_empty() {
            self.search.content_selected = Some(0);
        }
        if done {
            self.search.done = true;
            self.search.count = self.search.content_results.len();
        }
        cx.notify();
    }
    pub fn toggle_content_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .search
            .input
            .as_ref()
            .is_none_or(|input| input.read(cx).value().trim().is_empty())
        {
            return;
        }
        self.search.content_mode = !self.search.content_mode;
        self.search.content_results.clear();
        self.search.content_done = false;
        self.search.content_selected = None;
        self.search.content_error = None;
        self.search.content_scanned = 0;
        self.search.content_truncated = false;
        if self.search.content_mode {
            let value = self
                .search
                .input
                .as_ref()
                .map(|input| input.read(cx).value())
                .unwrap_or_default();
            self.start_content_search(value, cx);
        } else {
            self.search.content_token += 1;
            self.search
                .content_latest
                .store(self.search.content_token, Ordering::Release);
            let value = self
                .search
                .input
                .as_ref()
                .map(|input| input.read(cx).value())
                .unwrap_or_default();
            self.start_search(value, cx);
        }
        window.focus(
            &self
                .search
                .input
                .as_ref()
                .expect("search input")
                .focus_handle(cx),
            cx,
        );
        cx.notify();
    }
    pub fn reveal_content_result(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(found) = self.search.content_results.get(index).cloned() else {
            return;
        };
        let Some(parent) = found.path.parent() else {
            return;
        };
        let parent = parent.to_owned();
        let name = found
            .path
            .file_name()
            .map(std::ffi::OsString::from)
            .unwrap_or_default();
        self.close_search();
        self.navigate(parent, true, cx);
        self.files.locate_token += 1;
        self.files.locate_rename = false;
        self.source.locate(self.files.locate_token, name);
        self.files.focus_grid = true;
    }
    pub fn accept_search(&mut self, notice: Notice, cx: &mut Context<Self>) {
        if !self.search.open {
            return;
        }
        if let Notice::IndexUsed { root } = &notice {
            if let Some(index) = &self.index {
                let _ = index.send(crate::index::Action::Touch(*root));
            }
            return;
        }
        if let Notice::IndexCandidate { path } = &notice {
            if let Some(index) = &self.index {
                let _ = index.send(crate::index::Action::AddOpportunistic(path.clone()));
            }
            return;
        }
        if let Notice::Invalidated { token } = &notice {
            if *token == self.search.token {
                self.search.refreshing = false;
                self.search.refresh = None;
                self.search.done = true;
                self.refresh_search();
                cx.notify();
            }
            return;
        }
        if self.search.refreshing {
            match notice {
                Notice::Progress {
                    token,
                    count,
                    scanned,
                    unavailable,
                    skipped,
                    done,
                    error,
                } if token == self.search.token => {
                    if let Some(error) = error {
                        self.search.refreshing = false;
                        self.search.refresh = None;
                        self.navigation_error(error, cx);
                    } else if done {
                        self.search.refresh = Some(Notice::Progress {
                            token,
                            count,
                            scanned,
                            unavailable,
                            skipped,
                            done,
                            error: None,
                        });
                        let start = self.search.requested.start.min(count.saturating_sub(1));
                        self.search.serial += 1;
                        self.search.worker.page(
                            token,
                            self.search.serial,
                            start..(start + 32).min(count),
                        );
                    }
                }
                Notice::Page {
                    token,
                    serial,
                    rows,
                } if token == self.search.token
                    && serial == self.search.serial
                    && self.search.refresh.is_some() =>
                {
                    let selected = self
                        .search
                        .selected
                        .and_then(|id| self.search.rows.get(&id))
                        .map(|r| r.path.clone());
                    let Some(Notice::Progress {
                        count,
                        scanned,
                        unavailable,
                        skipped,
                        ..
                    }) = self.search.refresh.take()
                    else {
                        unreachable!()
                    };
                    self.search.selected = selected
                        .and_then(|path| rows.iter().find(|r| r.path == path).map(|r| r.id));
                    self.search.rows = rows.into_iter().map(|r| (r.id, r)).collect();
                    self.search.count = count;
                    self.search.scanned = scanned;
                    self.search.unavailable = unavailable;
                    self.search.skipped = skipped;
                    self.search.done = true;
                    self.search.refreshing = false;
                    self.search.error = None;
                    self.search.requested = 0..0;
                    if self.search.refresh_again {
                        self.search.refresh_again = false;
                        self.refresh_search();
                    }
                }
                _ => return,
            }
            cx.notify();
            return;
        }
        match notice {
            Notice::Progress {
                token,
                count,
                scanned,
                unavailable,
                skipped,
                done,
                error,
            } if token == self.search.token => {
                let grew = count != self.search.count;
                if done && !self.search.done {
                    self.search.rows.clear();
                    self.search.selected = None;
                    self.search.requested = 0..0;
                }
                self.search.count = count;
                self.search.scanned = scanned;
                self.search.unavailable = unavailable;
                self.search.skipped = skipped;
                self.search.done = done;
                self.search.error = error;
                if grew {
                    self.search.requested = 0..0;
                }
            }
            Notice::Page {
                token,
                serial,
                rows,
            } if token == self.search.token && serial == self.search.serial => {
                self.search.rows = rows.into_iter().map(|row| (row.id, row)).collect();
            }
            _ => return,
        }
        if self.search.done
            && (!self.search.rows.is_empty() || self.search.count == 0)
            && self.search.refresh_again
        {
            self.search.refresh_again = false;
            self.refresh_search();
        }
        cx.notify();
    }
    pub fn reveal_search_result(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self
            .search
            .selected
            .and_then(|id| self.search.rows.get(&id))
            .cloned()
            && let (Some(parent), Some(name)) = (row.path.parent(), row.path.file_name())
        {
            let (parent, name) = (parent.to_owned(), name.to_owned());
            self.close_search();
            self.navigate(parent, true, cx);
            self.files.locate_token += 1;
            self.files.locate_rename = false;
            self.source.locate(self.files.locate_token, name);
            self.files.focus_grid = true;
        }
    }
    pub fn search_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = e.keystroke.key.as_str();
        let m = e.keystroke.modifiers;
        if !m.platform && !m.alt && m.control && key == "f" && !self.files.close_guard {
            self.open_search(window, cx);
            return true;
        }
        if !self.search.open {
            return false;
        }
        if self
            .search
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).is_composing())
        {
            return false;
        }
        if self.view_inputs_focused(window, cx) {
            return false;
        }
        if key == "escape" {
            self.close_search_checked(window, cx);
            return true;
        }
        if m.alt || m.platform {
            return false;
        }
        match key {
            "f5" => {
                if self.search.definition.indexed
                    && let Some(index) = &self.index
                    && let Err(error) = index.send(crate::index::Action::Rescan)
                {
                    self.navigation_error(error, cx);
                }
                self.refresh_search();
            }
            "down" | "up" if !m.control => {
                if self.search.count > 0 {
                    let next = if key == "down" {
                        self.search
                            .selected
                            .map_or(0, |id| (id + 1).min(self.search.count - 1))
                    } else {
                        self.search
                            .selected
                            .map_or(self.search.count - 1, |id| id.saturating_sub(1))
                    };
                    self.search.selected = Some(next);
                    self.search
                        .scroll
                        .scroll_to_item(next / self.search.columns.max(1), ScrollStrategy::Top);
                }
            }
            "enter" => self.reveal_search_result(cx),
            "tab" => {
                if m.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }
    pub fn search_width(&self, window: &Window, cx: &App) -> f32 {
        let c = &layout::CHROME;
        let extra = if self.visible_operations().is_empty() {
            0.
        } else {
            64.
        };
        let target = (f32::from(window.viewport_size().width) - 360. - extra)
            .clamp(c.search_minimum_width, c.search_width);
        c.control_size
            + (target - c.control_size)
                * self
                    .search
                    .progress(self.options.reduced_motion || cx.reduce_motion())
    }
    pub fn search_entry(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        if let Some(input) = &self.search.input {
            let active = input.focus_handle(cx).is_focused(window);
            let input = input.clone();
            let focus_input = input.clone();
            return div()
                .id("expanded-search")
                .w(px(self.search_width(window, cx)))
                .h(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(6.))
                .rounded(px(8.))
                .bg(colors.color(ColorRole::Floating))
                .border_1()
                .border_color(colors.color(ColorRole::Border))
                .shadow(colors.control_shadows(active))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&focus_input.focus_handle(cx), cx);
                    cx.stop_propagation();
                })
                .child(icon("search", colors).flex_none())
                .child(div().flex_1().min_w_0().overflow_hidden().child(input))
                .into_any_element();
        }
        let fraction = self
            .search
            .progress(self.options.reduced_motion || cx.reduce_motion());
        let closing = !self.search.open && fraction > 0.;
        let available = !self.files.close_guard;
        div()
            .id("search-entry")
            .w(px(self.search_width(window, cx)))
            .h(px(layout::CHROME.control_size))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .role(Role::Button)
            .aria_label(crate::i18n::tr("搜索文件名，Ctrl+F"))
            .tooltip(tooltip(
                crate::i18n::tr_owned("搜索文件名 · Ctrl+F"),
                colors,
            ))
            .focusable()
            .tab_stop(true)
            .opacity(if available { 1. } else { 0.5 })
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(false))
            })
            .focus(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(true))
            })
            .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx)))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if paint::activation_key(e) {
                    cx.stop_propagation();
                    this.open_search(window, cx);
                }
            }))
            .when(closing, |d| {
                d.child(
                    div()
                        .absolute()
                        .size_full()
                        .rounded(px(8.))
                        .bg(colors.color(ColorRole::Floating))
                        .border_1()
                        .border_color(colors.color(ColorRole::Border))
                        .opacity(fraction),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(7.))
                    .top(px(7.))
                    .child(icon("search", colors)),
            )
            .when(closing, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(34.))
                        .right(px(8.))
                        .top_0()
                        .h_full()
                        .min_w_0()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .opacity(fraction)
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(self.search.closing_text.clone()),
                        ),
                )
            })
            .into_any_element()
    }
    pub fn content_search_overlay(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors;
        let viewport = window.viewport_size();
        let left = layout::content_left(self.sidebar_width())
            .min((f32::from(viewport.width) - 336.).max(0.));
        let top = layout::CHROME.bar_height + 8.;
        let width = (f32::from(viewport.width) - left - 16.).max(320.);
        let height = (f32::from(viewport.height) - top - 16.).max(240.);
        let inner_width = (width - 32.).max(1.);
        let gap = 12.;
        let minimum_card = 260.;
        let columns = (((inner_width + gap) / (minimum_card + gap)).floor() as usize).max(1);
        let card_width = ((inner_width - gap * (columns - 1) as f32) / columns as f32).max(1.);
        let card_height = 160.;
        let count = self.search.content_results.len();
        let rows = count.div_ceil(columns);
        let status = if let Some(error) = &self.search.content_error {
            error.clone()
        } else if !self.search.started {
            String::new()
        } else if !self.search.content_done {
            format!(
                "内容搜索中 · 已检查 {} 个文件",
                number(self.search.content_scanned)
            )
        } else if count == 0 {
            crate::i18n::tr_owned("没有匹配内容")
        } else {
            format!(
                "{} 个文件包含“{}”{}",
                number(count),
                self.search.query,
                if self.search.content_truncated {
                    crate::i18n::tr(" · 结果已截断")
                } else {
                    ""
                }
            )
        };
        let scope = format!(
            "内容 · {} · 含子文件夹",
            navigation::display_name(&self.search.root)
        );
        let empty = if let Some(error) = &self.search.content_error {
            error.clone()
        } else if !self.search.content_done {
            crate::i18n::tr_owned("正在扫描文件内容…")
        } else {
            crate::i18n::tr_owned("没有匹配内容")
        };
        let list = if rows > 0 {
            uniform_list(
                "content-results",
                rows,
                cx.processor(move |this, range: Range<usize>, window, cx| {
                    range
                        .map(|row| {
                            div()
                                .h(px(card_height))
                                .w_full()
                                .flex()
                                .gap(px(gap))
                                .children(
                                    (row * columns
                                        ..((row + 1) * columns)
                                            .min(this.search.content_results.len()))
                                        .map(|index| {
                                            let found = &this.search.content_results[index];
                                            let selected =
                                                this.search.content_selected == Some(index);
                                            let name = found.name.clone();
                                            let path = found.path.to_string_lossy().into_owned();
                                            let line = found.line;
                                            let matches = found.matches;
                                            let mut style = window.text_style();
                                            style.font_size = px(12.).into();
                                            style.line_height = px(18.).into();
                                            style.color = colors.color(ColorRole::Primary);
                                            let highlight = HighlightStyle {
                                                background_color: Some(
                                                    colors.color(ColorRole::InputActive),
                                                ),
                                                color: Some(colors.color(ColorRole::Primary)),
                                                ..Default::default()
                                            };
                                            let styled = StyledText::new(SharedString::from(
                                                found.snippet.clone(),
                                            ))
                                            .with_default_highlights(
                                                &style,
                                                found
                                                    .highlights
                                                    .iter()
                                                    .cloned()
                                                    .map(|range| (range, highlight)),
                                            );
                                            let icon = file_icons::FileIcon::for_name(
                                                std::ffi::OsStr::new(&name),
                                                false,
                                            )
                                            .asset(colors.dark)
                                            .into();
                                            div()
                                                .id(("content-card", index))
                                                .w(px(card_width))
                                                .h(px(card_height))
                                                .flex_none()
                                                .flex()
                                                .flex_col()
                                                .gap(px(5.))
                                                .px(px(10.))
                                                .py(px(8.))
                                                .rounded(px(10.))
                                                .when(selected, |d| {
                                                    d.bg(colors.color(ColorRole::InputActive))
                                                })
                                                .hover(move |d| {
                                                    d.bg(colors.color(ColorRole::Hover))
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.reveal_content_result(index, cx);
                                                    cx.stop_propagation();
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(7.))
                                                        .child(paint::file_artwork(
                                                            icon, 18., false, colors,
                                                        ))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .truncate()
                                                                .text_size(px(13.))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child(name),
                                                        )
                                                        .when(matches > 1, |d| {
                                                            d.child(
                                                                div()
                                                                    .text_size(px(11.))
                                                                    .text_color(colors.color(
                                                                        ColorRole::Secondary,
                                                                    ))
                                                                    .child(format!("×{matches}")),
                                                            )
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .h(px(14.))
                                                        .truncate()
                                                        .text_size(px(10.5))
                                                        .text_color(
                                                            colors.color(ColorRole::Secondary),
                                                        )
                                                        .child(crate::trf!(
                                                            "第 {} 行 · {}",
                                                            line,
                                                            path
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_h_0()
                                                        .overflow_hidden()
                                                        .child(styled),
                                                )
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>(),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .h_full()
            .track_scroll(&self.search.content_scroll)
            .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.color(ColorRole::Primary))
                        .child(empty),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(colors.color(ColorRole::Secondary))
                        .child(crate::i18n::tr(
                            "递归检查文本文件内容；可尝试更短、更具体的关键词",
                        )),
                )
                .into_any_element()
        };
        div()
            .absolute()
            .top(px(layout::CHROME.bar_height))
            .bottom_0()
            .left_0()
            .right_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_search_checked(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("content-search-panel")
                    .absolute()
                    .top(px(8.))
                    .left(px(left))
                    .w(px(width))
                    .h(px(height))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(14.))
                    .bg(colors.color(ColorRole::Floating))
                    .shadow(vec![colors.shadow()])
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.view_toolbar(cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(colors.color(ColorRole::Secondary))
                                    .child(scope),
                            )
                            .when(!status.is_empty(), |d| {
                                d.child(
                                    div()
                                        .truncate()
                                        .text_size(px(12.))
                                        .text_color(colors.color(
                                            if self.search.content_error.is_some() {
                                                ColorRole::Negative
                                            } else {
                                                ColorRole::Secondary
                                            },
                                        ))
                                        .child(status),
                                )
                            }),
                    )
                    .child(div().flex_1().min_h_0().child(list)),
            )
            .into_any_element()
    }
    pub fn search_overlay(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.search.content_mode {
            return self.content_search_overlay(window, cx);
        }
        let colors = self.colors;
        let width = (f32::from(window.viewport_size().width) - 156.).clamp(240., 560.);
        let height = (f32::from(window.viewport_size().height) - 192.).clamp(48., 384.);
        let popover = self.search_popover_visible(cx);
        let scope = if self.search.definition.indexed {
            crate::i18n::tr_owned("已索引位置")
        } else {
            format!(
                "{}{}",
                navigation::display_name(&self.search.root),
                if self.search.definition.recursive {
                    " · 含子文件夹"
                } else {
                    ""
                }
            )
        };
        let status = if let Some(error) = &self.search.error {
            error.clone()
        } else if !self.search.started {
            String::new()
        } else if !self.search.done {
            crate::trf!("搜索中 · 已检查 {} 项", number(self.search.scanned))
        } else if self.search.count == 0 {
            crate::i18n::tr_owned("没有匹配项")
        } else {
            crate::trf!("{} 项", number(self.search.count))
        };
        let index_status = if self.search.definition.indexed {
            if let Some(error) = &self.index_status.error {
                crate::trf!(" · 索引不可用：{}", error)
            } else {
                format!(
                    " · 已索引 {} 项{}{}",
                    number(self.index_status.count),
                    if self.index_status.building {
                        crate::i18n::tr(" · 正在建立索引")
                    } else {
                        ""
                    },
                    if self.index_status.unavailable > 0 {
                        crate::trf!(" · {} 个位置不可访问", self.index_status.unavailable)
                    } else {
                        String::new()
                    }
                )
            }
        } else {
            String::new()
        };
        let status = format!("{status}{index_status}");
        let status = format!(
            "{status}{}{}",
            if self.search.unavailable > 0 {
                crate::trf!(" · {} 个位置无法访问", self.search.unavailable)
            } else {
                String::new()
            },
            if self.search.skipped > 0 {
                crate::trf!(" · 未展开 {} 个链接", self.search.skipped)
            } else {
                String::new()
            }
        );
        let list = self.search_results(width, height, cx);
        div()
            .absolute()
            .top(px(layout::CHROME.bar_height))
            .bottom_0()
            .left_0()
            .right_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_search_checked(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .when(popover, |overlay| {
                overlay.child(
                    div()
                        .id("search-popover")
                        .absolute()
                        .top(px(4.))
                        .right(px(144.))
                        .w(px(width))
                        .px(px(12.))
                        .py(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .rounded(px(12.))
                        .bg(colors.color(ColorRole::Floating))
                        .border_1()
                        .border_color(colors.color(ColorRole::Border))
                        .shadow(vec![colors.shadow()])
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(self.view_toolbar(cx))
                        .child(
                            div()
                                .id("search-scope")
                                .truncate()
                                .text_size(px(12.))
                                .text_color(colors.color(ColorRole::Secondary))
                                .role(Role::Label)
                                .aria_value(scope.clone())
                                .tooltip(tooltip(
                                    self.search.root.to_string_lossy().into_owned(),
                                    colors,
                                ))
                                .child(scope),
                        )
                        .when(self.search.count > 0, |d| d.child(list))
                        .when(!status.is_empty(), |d| {
                            d.child(
                                div()
                                    .id("search-status")
                                    .text_size(px(12.))
                                    .text_color(colors.color(
                                        if self.search.error.is_some()
                                            || self.search.unavailable > 0
                                        {
                                            ColorRole::Negative
                                        } else {
                                            ColorRole::Secondary
                                        },
                                    ))
                                    .role(Role::Label)
                                    .aria_value(status.clone())
                                    .child(status),
                            )
                        }),
                )
            })
            .into_any_element()
    }
}
fn number(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
