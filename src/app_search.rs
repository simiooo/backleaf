use super::*;
use crate::{
    input::{InputEvent, TextInput},
    search::{Notice, Row, Worker},
};
use std::collections::BTreeMap;

pub struct SearchUi {
    worker: Worker,
    pub open: bool,
    motion: crate::search_motion::SearchMotion,
    closing_text: String,
    input: Option<Entity<TextInput>>,
    subscription: Option<Subscription>,
    token: u64,
    serial: u64,
    requested: Range<usize>,
    rows: BTreeMap<usize, Row>,
    count: usize,
    scanned: usize,
    unavailable: usize,
    skipped: usize,
    done: bool,
    started: bool,
    query: String,
    root: PathBuf,
    error: Option<String>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
}
impl SearchUi {
    pub fn new(worker: Worker) -> Self {
        Self {
            worker,
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
            query: String::new(),
            root: PathBuf::new(),
            error: None,
            selected: None,
            scroll: UniformListScrollHandle::new(),
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
    pub fn close_search(&mut self) {
        self.search.open = false;
        self.search.motion.retarget(
            false,
            Instant::now(),
            Duration::from_millis(layout::CHROME.search_close_duration_ms),
        );
        self.search.token += 1;
        self.search.worker.cancel(self.search.token);
        self.search.input = None;
        self.search.subscription = None;
        self.search.rows.clear();
    }
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.close_guard || self.options.fixture.is_some() {
            return;
        }
        if self.defer_search(cx) {
            return;
        }
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
        self.search.query.clear();
        self.search.started = false;
        self.search.count = 0;
        self.search.error = None;
        self.search.selected = None;
        self.search.scroll = UniformListScrollHandle::new();
        let input = cx.new(|cx| {
            let mut input =
                TextInput::new(String::new(), 0..0, "搜索文件名", self.colors, 1024, cx);
            input.set_placeholder("搜索文件名");
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
                        this.search.token += 1;
                        this.search.worker.cancel(this.search.token);
                        this.search.rows.clear();
                        this.search.count = 0;
                        this.search.selected = None;
                        this.search.started = false;
                        this.search.error = None;
                        this.search.requested = 0..0;
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
                    InputEvent::Submit(value) => {
                        if this.search.query == value.trim() && this.search.selected.is_some() {
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
    fn start_search(&mut self, value: String, cx: &mut Context<Self>) {
        let query = value.trim().to_owned();
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
        self.search.started = !query.is_empty();
        self.search.done = false;
        if !query.is_empty() {
            self.search
                .worker
                .search(self.search.token, self.search.root.clone(), query);
        }
        cx.notify();
    }
    pub fn accept_search(&mut self, notice: Notice, cx: &mut Context<Self>) {
        if !self.search.open {
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
        cx.notify();
    }
    fn reveal_search_result(&mut self, cx: &mut Context<Self>) {
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
        if key == "escape" {
            self.close_search();
            window.focus(&self.focus, cx);
            cx.notify();
            return true;
        }
        if m.alt || m.platform {
            return false;
        }
        match key {
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
                    self.search.scroll.scroll_to_item(next, ScrollStrategy::Top);
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
        let extra = if self.files.hub.views().is_empty() {
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
                    window.focus(&focus_input.focus_handle(cx), cx)
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
            .aria_label("搜索文件名，Ctrl+F")
            .tooltip(tooltip("搜索文件名 · Ctrl+F".into(), colors))
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
    pub fn search_overlay(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let width = (f32::from(window.viewport_size().width) - 156.).clamp(240., 560.);
        let height = (f32::from(window.viewport_size().height) - 192.).clamp(48., 384.);
        let scope = format!(
            "{} · 含子文件夹",
            navigation::display_name(&self.search.root)
        );
        let status = if let Some(error) = &self.search.error {
            error.clone()
        } else if !self.search.started {
            String::new()
        } else if !self.search.done {
            format!("搜索中 · 已检查 {} 项", number(self.search.scanned))
        } else if self.search.count == 0 {
            "没有匹配项".into()
        } else {
            format!("{} 项", number(self.search.count))
        };
        let status = format!(
            "{status}{}{}",
            if self.search.unavailable > 0 {
                format!(" · {} 个位置无法访问", self.search.unavailable)
            } else {
                String::new()
            },
            if self.search.skipped > 0 {
                format!(" · 未展开 {} 个链接", self.search.skipped)
            } else {
                String::new()
            }
        );
        let list = uniform_list(
            "search-results",
            self.search.count,
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                if range.len() == 1 && this.search.count > 1 && height > 48. {
                    return vec![div().h(px(48.)).w_full().into_any_element()];
                }
                let requested = range.start..range.end.min(range.start.saturating_add(32));
                if this.search.requested != requested {
                    this.search.requested = requested.clone();
                    this.search.serial += 1;
                    this.search
                        .worker
                        .page(this.search.token, this.search.serial, requested);
                }
                range
                    .map(|id| {
                        let row = this.search.rows.get(&id);
                        let name = row.map_or("…", |r| r.name.as_str()).to_owned();
                        let path = row
                            .map(|r| navigation::display_path(&r.path))
                            .unwrap_or_default();
                        let parent = row.map(|r| r.parent.clone()).unwrap_or_default();
                        let ready = row.is_some();
                        let artwork = row.map_or(crate::file_icons::FileIcon::Generic, |r| {
                            crate::file_icons::FileIcon::for_name(
                                r.path.file_name().unwrap_or_default(),
                                r.directory,
                            )
                        });
                        let native = if artwork == crate::file_icons::FileIcon::Application {
                            row.and_then(|r| this.application_icons.image(r.path.clone()))
                        } else {
                            None
                        };
                        let outlined =
                            native.is_none() && artwork != crate::file_icons::FileIcon::Folder;
                        let artwork = native.unwrap_or_else(|| artwork.asset(colors.dark).into());
                        div()
                            .id(("search-result", id))
                            .h(px(48.))
                            .w_full()
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(8.))
                            .when(this.search.selected == Some(id), |d| {
                                d.bg(colors.color(ColorRole::Hover))
                            })
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .role(Role::Button)
                            .aria_label(format!("在文件夹中显示 {name}，{path}"))
                            .tooltip(tooltip(path, colors))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if ready {
                                    this.search.selected = Some(id);
                                    this.reveal_search_result(cx);
                                }
                            }))
                            .child(paint::file_artwork(artwork, 24., outlined, colors))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().truncate().child(name))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.))
                                            .text_color(colors.color(ColorRole::Secondary))
                                            .child(parent),
                                    ),
                            )
                            .child(icon("reveal", colors))
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .h(px(height.min(self.search.count as f32 * 48.)))
        .track_scroll(&self.search.scroll);
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
                    this.close_search();
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .when(
                self.search.started || self.search.error.is_some(),
                |overlay| {
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
                },
            )
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
