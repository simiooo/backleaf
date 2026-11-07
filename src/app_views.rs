use super::*;
use crate::{
    file_view::{FileView, Group, Layout, Sort},
    input::{InputEvent, TextInput},
    quick_access::{self, Destination, Entry},
};
#[derive(Clone, Copy)]
enum Field {
    Extensions,
    Days,
    Min,
    Max,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Extensions => "类型",
            Self::Days => "最近天数",
            Self::Min => "最小字节",
            Self::Max => "最大字节",
        }
    }
}
pub struct SaveEditor {
    input: Entity<TextInput>,
    _subscription: Subscription,
    definition: FileView,
    previous: Option<Entry>,
    pending: bool,
    token: u64,
}
struct FilterInput {
    field: Field,
    input: Entity<TextInput>,
    _subscription: Subscription,
}
#[derive(Default)]
pub struct ViewsUi {
    editor: Option<SaveEditor>,
    filters: Option<Vec<FilterInput>>,
    focus: Option<Entity<TextInput>>,
    serial: u64,
    close_after_save: bool,
}
impl Backleaf {
    pub fn view_inputs_pending(&self, cx: &App) -> bool {
        self.views
            .editor
            .as_ref()
            .is_some_and(|e| e.pending || e.input.read(cx).is_pasting())
            || self
                .views
                .filters
                .as_ref()
                .is_some_and(|fields| fields.iter().any(|f| f.input.read(cx).is_pasting()))
    }
    pub fn flush_view_fields(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(e) = &self.views.editor {
            if e.input.read(cx).is_composing() {
                self.views.focus = Some(e.input.clone());
                self.restore.close_requested = false;
                self.navigation_error("请先完成输入法候选".into(), cx);
                return false;
            }
            self.restore.close_requested = true;
            if !e.pending && !e.input.read(cx).is_pasting() {
                let value = e.input.read(cx).value();
                self.commit_saved_view(value, cx);
            }
            return false;
        }
        if self.views.filters.is_some() {
            if !self.apply_view_filters(cx) {
                self.restore.close_requested = false;
                return false;
            }
            self.views.filters = None;
        }
        true
    }
    pub fn view_inputs_focused(&self, window: &Window, cx: &App) -> bool {
        self.views
            .editor
            .as_ref()
            .is_some_and(|e| e.input.focus_handle(cx).is_focused(window))
            || self.views.filters.as_ref().is_some_and(|fields| {
                fields
                    .iter()
                    .any(|f| f.input.focus_handle(cx).is_focused(window))
            })
    }
    pub fn view_editing(&self) -> bool {
        self.views.editor.is_some() || self.views.filters.is_some()
    }
    pub fn close_search_checked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.views.filters.is_some() {
            if !self.apply_view_filters(cx) {
                return;
            }
            self.views.filters = None;
        }
        if let Some(e) = &self.views.editor {
            if e.input.read(cx).is_composing() || e.input.read(cx).is_pasting() {
                self.views.focus = Some(e.input.clone());
                self.navigation_error("请先完成输入".into(), cx);
                return;
            }
            let value = e.input.read(cx).value();
            self.views.close_after_save = true;
            self.commit_saved_view(value, cx);
            return;
        }
        self.close_search();
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub fn accept_quick_access(&mut self, notice: quick_access::Notice, cx: &mut Context<Self>) {
        match notice {
            quick_access::Notice::Loaded { revision, entries } => {
                self.navigation.config_revision = revision;
                if self.options.fixture.is_none() {
                    // Saved views that search indexed locations pin their covering
                    // roots against opportunistic LRU eviction.
                    let pins = entries
                        .iter()
                        .filter_map(|entry| match &entry.destination {
                            Destination::View(view) => Some(view.root.path()),
                            Destination::Folder(_) => None,
                        })
                        .collect::<Vec<_>>();
                    if let Some(index) = &self.index {
                        let _ = index.send(crate::index::Action::SetPins(pins));
                    }
                    self.navigation.common = entries
                        .into_iter()
                        .map(|e| {
                            let path = match &e.destination {
                                Destination::Folder(p) => p.path(),
                                Destination::View(v) => v.root.path(),
                            };
                            navigation::Location {
                                title: e.title.clone(),
                                path,
                                volume: false,
                                capacity: None,
                                entry: Some(e),
                            }
                        })
                        .collect();
                }
            }
            quick_access::Notice::Saved { token, result } => {
                if self.views.editor.as_ref().is_some_and(|e| e.token == token) {
                    match result {
                        Ok(entry) => {
                            self.search.saved = Some(entry);
                            self.views.editor = None;
                            if self.views.close_after_save {
                                self.views.close_after_save = false;
                                self.close_search();
                                self.files.focus_grid = true;
                            }
                        }
                        Err(error) => {
                            if let Some(e) = &mut self.views.editor {
                                e.pending = false;
                                e.input.update(cx, |i, cx| {
                                    i.enabled = true;
                                    cx.notify();
                                });
                                self.views.focus = Some(e.input.clone());
                            }
                            self.views.close_after_save = false;
                            self.restore.close_requested = false;
                            self.navigation_error(error, cx);
                        }
                    }
                }
            }
            quick_access::Notice::Error(error) => self.navigation_error(error, cx),
        }
        cx.notify();
    }
    pub fn open_saved_view(
        &mut self,
        entry: Entry,
        definition: FileView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.defer_saved_view(entry.clone(), definition.clone(), cx) {
            return;
        }
        self.open_search(window, cx);
        self.search.root = definition.root.path();
        self.search.definition = definition.clone();
        self.search.saved = Some(entry);
        if let Some(input) = &self.search.input {
            input.update(cx, |i, cx| {
                i.restore_unconfirmed_composition(definition.query.clone(), cx)
            });
        }
        self.start_search(definition.query, cx);
    }
    pub fn begin_save_view(&mut self, as_new: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_typeahead();
        self.views.filters = None;
        let previous = if as_new {
            None
        } else {
            self.search.saved.clone()
        };
        let title = previous
            .as_ref()
            .map(|e| e.title.clone())
            .unwrap_or_else(|| {
                if self.search.query.is_empty() {
                    "搜索".into()
                } else {
                    self.search.query.chars().take(80).collect()
                }
            });
        let input = cx.new(|cx| {
            TextInput::new(
                title.clone(),
                0..title.len(),
                "视图名称",
                self.colors,
                80,
                cx,
            )
        });
        let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            match event {
                InputEvent::Submit(value) | InputEvent::Blur(value) => {
                    if input.read(cx).is_composing() {
                        this.views.focus = Some(input.clone());
                        this.navigation_error("请先完成输入法候选".into(), cx);
                    } else {
                        this.commit_saved_view(value.clone(), cx);
                    }
                }
                InputEvent::Cancel => {
                    this.views.editor = None;
                }
                InputEvent::Error(error) => {
                    this.views.focus = Some(input.clone());
                    this.navigation_error(error.clone(), cx);
                }
                _ => {}
            }
            cx.notify();
        });
        self.views.serial += 1;
        self.views.editor = Some(SaveEditor {
            input: input.clone(),
            _subscription: subscription,
            definition: self.search.definition.clone(),
            previous,
            pending: false,
            token: self.views.serial,
        });
        window.focus(&input.focus_handle(cx), cx);
        cx.notify();
    }
    fn commit_saved_view(&mut self, title: String, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.views.editor else {
            return;
        };
        if editor.pending {
            return;
        }
        let request = quick_access::Action::Save {
            token: editor.token,
            id: editor.previous.as_ref().map(|e| e.id.clone()),
            revision: editor.previous.as_ref().map_or(0, |e| e.revision),
            title,
            view: editor.definition.clone(),
        };
        editor.pending = true;
        editor.input.update(cx, |i, cx| {
            i.enabled = false;
            cx.notify();
        });
        if let Err(error) = self.navigation.store.send(request) {
            let e = self.views.editor.as_mut().unwrap();
            e.pending = false;
            e.input.update(cx, |i, cx| {
                i.enabled = true;
                cx.notify();
            });
            self.views.focus = Some(e.input.clone());
            self.restore.close_requested = false;
            self.navigation_error(error, cx);
        }
    }
    pub fn toggle_view_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.views.filters.is_some() {
            if self.apply_view_filters(cx) {
                self.views.filters = None;
            }
            return;
        }
        let d = &self.search.definition;
        let values = [
            (Field::Extensions, d.extensions.join(", ")),
            (
                Field::Days,
                d.modified_days.map(|v| v.to_string()).unwrap_or_default(),
            ),
            (
                Field::Min,
                d.min_bytes.map(|v| v.to_string()).unwrap_or_default(),
            ),
            (
                Field::Max,
                d.max_bytes.map(|v| v.to_string()).unwrap_or_default(),
            ),
        ];
        let mut fields = Vec::new();
        for (field, value) in values {
            let input = cx.new(|cx| {
                TextInput::new(
                    value.clone(),
                    0..value.len(),
                    field.label(),
                    self.colors,
                    256,
                    cx,
                )
            });
            let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                match event {
                    InputEvent::Blur(_) | InputEvent::Submit(_) => {
                        if input.read(cx).is_composing() {
                            this.views.focus = Some(input.clone());
                            this.navigation_error("请先完成输入法候选".into(), cx);
                        } else {
                            this.apply_view_filters(cx);
                        }
                    }
                    InputEvent::Cancel => {
                        this.views.filters = None;
                    }
                    InputEvent::Error(error) => {
                        this.views.focus = Some(input.clone());
                        this.navigation_error(error.clone(), cx);
                    }
                    _ => {}
                }
                cx.notify();
            });
            fields.push(FilterInput {
                field,
                input,
                _subscription: subscription,
            });
        }
        window.focus(&fields[0].input.focus_handle(cx), cx);
        self.views.filters = Some(fields);
        cx.notify();
    }
    fn apply_view_filters(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(fields) = &self.views.filters else {
            return true;
        };
        let mut definition = self.search.definition.clone();
        for item in fields {
            let text = item.input.read(cx).value();
            let text = text.trim();
            let result = (|| -> Result<(), String> {
                match item.field {
                    Field::Extensions => {
                        definition.extensions = text
                            .split([',', '，', ' '])
                            .filter(|s| !s.is_empty())
                            .map(|s| s.trim_start_matches('.').to_lowercase())
                            .collect()
                    }
                    Field::Days => {
                        definition.modified_days = if text.is_empty() {
                            None
                        } else {
                            Some(text.parse().map_err(|_| "天数无效")?)
                        }
                    }
                    Field::Min => {
                        definition.min_bytes = if text.is_empty() {
                            None
                        } else {
                            Some(text.parse().map_err(|_| "大小无效")?)
                        }
                    }
                    Field::Max => {
                        definition.max_bytes = if text.is_empty() {
                            None
                        } else {
                            Some(text.parse().map_err(|_| "大小无效")?)
                        }
                    }
                }
                definition.validate()
            })();
            if let Err(error) = result {
                let input = item.input.clone();
                self.views.focus = Some(input);
                self.navigation_error(error, cx);
                return false;
            }
        }
        if definition != self.search.definition {
            self.search.definition = definition;
            self.start_search(self.search.query.clone(), cx);
        }
        true
    }
    fn cycle_view_option(&mut self, key: usize, cx: &mut Context<Self>) {
        match key {
            0 => {
                self.search.definition.sort = match self.search.definition.sort {
                    Sort::Name => Sort::Modified,
                    Sort::Modified => Sort::Size,
                    Sort::Size => Sort::Name,
                }
            }
            1 => {
                self.search.definition.group = match self.search.definition.group {
                    Group::None => Group::Type,
                    Group::Type => Group::Date,
                    Group::Date => Group::None,
                };
                if self.search.definition.group != Group::None {
                    self.search.definition.layout = Layout::List;
                }
            }
            2 => self.search.definition.recursive = !self.search.definition.recursive,
            3 => {
                if self.search.definition.group == Group::None {
                    self.search.definition.layout = if self.search.definition.layout == Layout::List
                    {
                        Layout::Grid
                    } else {
                        Layout::List
                    };
                }
            }
            4 => self.search.definition.descending = !self.search.definition.descending,
            5 => self.search.definition.indexed = !self.search.definition.indexed,
            _ => {}
        }
        self.start_search(self.search.query.clone(), cx);
    }
    pub fn sync_view_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.views.focus.take() {
            window.focus(&input.focus_handle(cx), cx);
        }
    }
    pub fn search_results(
        &mut self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grid = self.search.definition.layout == Layout::Grid
            && self.search.definition.group == Group::None;
        let columns = if grid {
            ((width - 24.) / 112.).floor().max(1.) as usize
        } else {
            1
        };
        self.search.columns = columns;
        let result_layout = &layout::LAYOUT.search_result;
        let step = if grid {
            112.
        } else if self.search.definition.group != Group::None {
            result_layout.grouped_row_height
        } else {
            result_layout.row_height
        };
        let count = self.search.count;
        let total = count.div_ceil(columns);
        let colors = self.colors;
        uniform_list(
            "search-results",
            total,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                if range.len() == 1 && total > 1 && height > step {
                    return vec![div().h(px(step)).w_full().into_any_element()];
                }
                let requested = range.start * columns
                    ..(range.end * columns)
                        .min(count)
                        .min(range.start * columns + 32);
                if this.search.requested != requested {
                    this.search.requested = requested.clone();
                    let range = if !this.search.refreshing {
                        Some(requested)
                    } else if let Some(crate::search::Notice::Progress { count, .. }) =
                        &this.search.refresh
                    {
                        let start = requested.start.min(count.saturating_sub(1));
                        Some(start..(start + 32).min(*count))
                    } else {
                        None
                    };
                    if let Some(range) = range {
                        this.search.serial += 1;
                        this.search
                            .worker
                            .page(this.search.token, this.search.serial, range);
                    }
                }
                range
                    .map(|row_index| {
                        div()
                            .w_full()
                            .h(px(step))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(
                                (row_index * columns..((row_index + 1) * columns).min(count))
                                    .map(|id| {
                                        let row = this.search.rows.get(&id);
                                        let name = row
                                            .map(|r| r.name.clone())
                                            .unwrap_or_else(|| "…".into());
                                        let ready = row.is_some();
                                        let full = row
                                            .map(|r| navigation::display_path(&r.path))
                                            .unwrap_or_default();
                                        let parent =
                                            row.map(|r| r.parent.clone()).unwrap_or_default();
                                        let group = row.and_then(|r| r.group_heading.clone());
                                        let art = row.map_or(file_icons::FileIcon::Generic, |r| {
                                            file_icons::FileIcon::for_name(
                                                r.path.file_name().unwrap_or_default(),
                                                r.directory,
                                            )
                                        });
                                        let native = if art == file_icons::FileIcon::Application {
                                            row.and_then(|r| {
                                                this.application_icons.image(r.path.clone())
                                            })
                                        } else {
                                            None
                                        };
                                        let outlined =
                                            native.is_none() && art != file_icons::FileIcon::Folder;
                                        let image =
                                            native.unwrap_or_else(|| art.asset(colors.dark).into());
                                        let title = div()
                                            .id(("result-name", id))
                                            .truncate()
                                            .tooltip(tooltip(full, colors))
                                            .child(name.clone());
                                        let body = if grid {
                                            div()
                                                .size_full()
                                                .flex()
                                                .flex_col()
                                                .items_center()
                                                .justify_center()
                                                .gap(px(8.))
                                                .child(paint::file_artwork(
                                                    image, 44., outlined, colors,
                                                ))
                                                .child(title)
                                                .into_any_element()
                                        } else {
                                            div()
                                                .w_full()
                                                .h_full()
                                                .flex()
                                                .flex_col()
                                                .justify_center()
                                                .gap(px(result_layout.text_gap))
                                                .children(group.map(|g| {
                                                    div()
                                                        .h(px(12.))
                                                        .flex_none()
                                                        .text_size(px(11.))
                                                        .line_height(px(12.))
                                                        .text_color(
                                                            colors.color(ColorRole::Secondary),
                                                        )
                                                        .child(g)
                                                }))
                                                .child(
                                                    div()
                                                        .h(px(result_layout.name_line_height
                                                            + result_layout.parent_line_height
                                                            + result_layout.text_gap))
                                                        .flex_none()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(8.))
                                                        .child(paint::file_artwork(
                                                            image, 24., outlined, colors,
                                                        ))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .flex()
                                                                .flex_col()
                                                                .items_start()
                                                                .gap(px(result_layout.text_gap))
                                                                .child(
                                                                    title
                                                                        .max_w_full()
                                                                        .h(px(result_layout
                                                                            .name_line_height))
                                                                        .line_height(px(
                                                                            result_layout
                                                                                .name_line_height,
                                                                        ))
                                                                        .text_size(px(13.)),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .w_full()
                                                                        .truncate()
                                                                        .h(px(result_layout
                                                                            .parent_line_height))
                                                                        .line_height(px(
                                                                            result_layout
                                                                                .parent_line_height,
                                                                        ))
                                                                        .text_size(px(11.))
                                                                        .text_color(colors.color(
                                                                            ColorRole::Secondary,
                                                                        ))
                                                                        .child(parent),
                                                                ),
                                                        ),
                                                )
                                                .into_any_element()
                                        };
                                        div()
                                            .id(("search-result", id))
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(step))
                                            .px(px(result_layout.padding_x))
                                            .rounded(px(8.))
                                            .role(Role::ListBoxOption)
                                            .aria_label(name)
                                            .aria_selected(this.search.selected == Some(id))
                                            .when(this.search.selected == Some(id), |d| {
                                                d.bg(colors.color(ColorRole::Selected))
                                            })
                                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if ready {
                                                    this.search.selected = Some(id);
                                                    this.reveal_search_result(cx);
                                                }
                                            }))
                                            .child(body)
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .h(px(height.min(total as f32 * step)))
        .track_scroll(&self.search.scroll)
        .into_any_element()
    }
    pub fn view_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        div().flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().truncate().child(self.search.saved.as_ref().map(|e|format!("{}{}",e.title,if matches!(&e.destination,Destination::View(v)if *v!=self.search.definition){" · 已修改"}else{""})).unwrap_or_default()))
            .child(self.button("filter","筛选",cx.listener(|this,_,window,cx|this.toggle_view_filters(window,cx))))
            .child(self.button("save",if self.search.saved.is_some(){"更新视图"}else{"保存视图"},cx.listener(|this,_,window,cx|this.begin_save_view(false,window,cx))))
            .when(self.search.saved.is_some(),|d|d.child(self.button("copy","另存为",cx.listener(|this,_,window,cx|this.begin_save_view(true,window,cx)))))
            .into_any_element()
    }
    pub fn view_popovers(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let mut body = div().flex().flex_col().gap(px(8.));
        if let Some(editor) = &self.views.editor {
            body = body.child("名称").child(editor.input.clone());
        } else if let Some(fields) = &self.views.filters {
            for item in fields {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(72.))
                                .text_color(colors.color(ColorRole::Secondary))
                                .child(item.field.label()),
                        )
                        .child(div().flex_1().min_w_0().child(item.input.clone())),
                );
            }
            let d = &self.search.definition;
            for (key, label, value) in [
                (
                    0,
                    "排序",
                    match d.sort {
                        Sort::Name => "名称",
                        Sort::Modified => "修改日期",
                        Sort::Size => "大小",
                    },
                ),
                (
                    1,
                    "分组",
                    match d.group {
                        Group::None => "无",
                        Group::Type => "类型",
                        Group::Date => "日期",
                    },
                ),
                (
                    2,
                    "范围",
                    if d.recursive {
                        "含子文件夹"
                    } else {
                        "当前文件夹"
                    },
                ),
                (
                    3,
                    "布局",
                    if d.layout == Layout::List {
                        "列表"
                    } else {
                        "网格"
                    },
                ),
                (4, "顺序", if d.descending { "降序" } else { "升序" }),
                (
                    5,
                    "位置",
                    if d.indexed {
                        "已索引位置"
                    } else {
                        "当前路径"
                    },
                ),
            ] {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(72.))
                                .text_color(colors.color(ColorRole::Secondary))
                                .child(label),
                        )
                        .child(
                            div()
                                .id(("view-option", key as usize))
                                .flex_1()
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(8.))
                                .role(Role::Button)
                                .aria_label(format!("{label}：{value}"))
                                .focusable()
                                .tab_stop(true)
                                .hover(move |s| {
                                    s.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(false))
                                })
                                .focus_visible(move |s| {
                                    s.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(true))
                                })
                                .on_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, _, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            this.cycle_view_option(key as usize, cx);
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.cycle_view_option(key as usize, cx);
                                }))
                                .child(value),
                        ),
                );
            }
            if d.indexed {
                body = body.child(
                    div()
                        .id("indexed-roots")
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .children(
                            self.index_status
                                .roots
                                .iter()
                                .enumerate()
                                .map(|(i, path)| {
                                    let remove = path.clone();
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .id(("indexed-root-path", i))
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .tooltip(tooltip(
                                                    navigation::display_path(path),
                                                    colors,
                                                ))
                                                .child(navigation::display_path(path)),
                                        )
                                        .child(self.button(
                                            "close",
                                            "移出索引范围",
                                            cx.listener(move |this, _, _, cx| {
                                                if let Some(index) = &this.index
                                                    && let Err(error) =
                                                        index.send(crate::index::Action::Remove(
                                                            remove.clone(),
                                                        ))
                                                {
                                                    this.navigation_error(error, cx);
                                                }
                                            }),
                                        ))
                                        .id(("indexed-root", i))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>(),
                        ),
                );
                body = body.child(self.button(
                    "plus",
                    "索引当前文件夹",
                    cx.listener(|this, _, _, cx| {
                        if let Some(index) = &this.index
                            && let Err(error) =
                                index.send(crate::index::Action::Add(this.path.clone()))
                        {
                            this.navigation_error(error, cx);
                        }
                    }),
                ));
            }
        } else {
            return div().into_any_element();
        }
        div()
            .absolute()
            .right(px(144.))
            .top(px(layout::CHROME.bar_height + 52.))
            .w(px(360.))
            .max_w_full()
            .px(px(12.))
            .py(px(16.))
            .rounded(px(12.))
            .bg(colors.color(ColorRole::Floating))
            .shadow(colors.control_shadows(false))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(body)
            .into_any_element()
    }
}
