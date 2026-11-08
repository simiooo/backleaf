use super::*;
fn capacity_meter(
    value: Option<crate::volume_capacity::Capacity>,
    colors: &'static Palette,
) -> AnyElement {
    let spec = &layout::SCROLL;
    let label = value.map_or("—".into(), |v| v.label());
    div()
        .w_full()
        .h(px(spec.volume_meter_height))
        .relative()
        .flex_none()
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .w_full()
                .h(px(spec.volume_caption_line_height))
                .line_height(px(spec.volume_caption_line_height))
                .text_size(px(spec.volume_caption_font_size))
                .text_color(colors.color(ColorRole::Secondary))
                .truncate()
                .child(label),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .w_full()
                .h(px(spec.volume_meter_thickness))
                .rounded(px(spec.volume_meter_thickness / 2.))
                .overflow_hidden()
                .bg(colors.color(ColorRole::Border))
                .when_some(value, |d, v| {
                    d.child(
                        div()
                            .h_full()
                            .w(relative(v.fraction()))
                            .bg(colors.color(ColorRole::Secondary))
                            .opacity(0.5),
                    )
                }),
        )
        .into_any_element()
}
use crate::navigation::{Location, Notice, TreeRow, Worker};
pub struct NavigationUi {
    pub worker: Worker,
    pub store: crate::quick_access::Store,
    pub config_revision: u64,
    pub resolver: crate::locations::Resolver,
    pub volume_worker: Option<crate::locations::Volumes>,
    pub window_active: bool,
    pub menu_focus_transfer: bool,
    pub volumes: Vec<Location>,
    pub menu: Option<crate::menu::OwnedMenu>,
    pub pending: Option<u64>,
    pub(super) menu_subscription: Option<Subscription>,
    pub drag: Option<crate::app_quick_drag::QuickDrag>,
    pub suppress_click: bool,
    pub drag_tick: Instant,
    toast: Option<String>,
    toast_timer: Option<Task<()>>,
    pub common: Vec<Location>,
    pub rows: Vec<TreeRow>,
    pub open: bool,
    pub pinned: bool,
    pub token: u64,
    pub current: usize,
    pub truncated: bool,
    pub error: Option<String>,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
    pub hovered: bool,
}
impl NavigationUi {
    pub fn new(
        worker: Worker,
        tx: async_channel::Sender<Event>,
        real: bool,
        state_dir: PathBuf,
        cx: &mut Context<Backleaf>,
    ) -> Self {
        Self {
            worker,
            store: crate::quick_access::Store::new(state_dir, tx.clone()),
            config_revision: 0,
            resolver: crate::locations::Resolver::new(tx.clone()),
            volume_worker: real.then(|| crate::locations::Volumes::new(tx)),
            window_active: true,
            menu_focus_transfer: false,
            volumes: vec![],
            menu: None,
            pending: None,
            menu_subscription: None,
            drag: None,
            suppress_click: false,
            drag_tick: Instant::now(),
            toast: None,
            toast_timer: None,
            common: vec![],
            rows: vec![],
            open: false,
            pinned: false,
            token: 0,
            current: 0,
            truncated: false,
            error: None,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            hovered: false,
        }
    }
}
impl Backleaf {
    pub fn request_location(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.defer_location(path.clone(), cx) {
            return;
        }
        self.close_path_tree();
        self.navigation.menu = None;
        self.detail_token += 1;
        self.navigation.pending = Some(self.detail_token);
        self.navigation
            .resolver
            .resolve_exact(self.detail_token, path);
        cx.notify();
    }
    pub fn navigation_error(&mut self, message: String, cx: &mut Context<Self>) {
        let message = crate::i18n::tr_owned(&message);
        self.navigation.toast = Some(message);
        self.navigation.toast_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                this.navigation.toast = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn show_crumb_menu(
        &mut self,
        path: PathBuf,
        position: Point<Pixels>,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::menu::{Command, FileMenu, MenuEvent};
        if self.options.fixture.is_some() || self.files.close_guard {
            return;
        }
        if self.defer_crumb_menu(path.clone(), position, keyboard, cx) {
            return;
        }
        self.reset_typeahead();
        self.close_path_tree();
        self.files.menu = None;
        let menu = cx.new(|cx| FileMenu::breadcrumb(self.colors, position, keyboard, cx));
        let generation = self.generation;
        let restore = window.focused(cx);
        self.navigation.menu_subscription = Some(cx.subscribe_in(
            &menu,
            window,
            move |this, source, event: &MenuEvent, window, cx| {
                if this.navigation.menu.as_ref().map(|m| m.entity_id()) != Some(source.entity_id())
                {
                    return;
                }
                this.navigation.menu = None;
                if let Some(focus) = &restore {
                    window.focus(focus, cx);
                }
                if generation == this.generation
                    && let MenuEvent::Command(command) = event
                {
                    let command = *command;
                    let path = path.clone();
                    this.defer_menu_action(window, cx, move |this, window, cx| match command {
                        Command::CopyPath => {
                            if this.files.hub.clipboard_preparing() {
                                this.navigation_error(
                                    crate::i18n::tr_owned("正在准备剪贴板，请稍后重试"),
                                    cx,
                                );
                                return;
                            }
                            if let Err(error) =
                                crate::locations::copy_path(path.clone(), this.tx.clone())
                            {
                                this.navigation_error(error, cx);
                            }
                        }
                        Command::EditPath => this.edit_path_at(path.clone(), window, cx),
                        _ => (),
                    });
                }
                cx.notify();
            },
        ));
        self.files.focus_grid = false;
        let (owned, released) = crate::menu::OwnedMenu::new(menu.clone());
        self.navigation.menu = Some(owned);
        if let Err(error) =
            crate::popup_menu::open(menu, released, window, cx.entity().downgrade(), cx)
        {
            self.navigation.menu = None;
            self.navigation_error(error.to_string(), cx);
        }
        cx.notify();
    }
    pub fn navigation_overlay(&self, window: &Window) -> AnyElement {
        let colors = self.colors;
        div()
            .absolute()
            .inset_0()
            .when_some(self.navigation.toast.clone(), |d, error| {
                d.child(
                    div()
                        .id("navigation-error-toast")
                        .absolute()
                        .left(px(80.))
                        .top(px(layout::CHROME.bar_height + 4.))
                        .w(px(
                            280_f32.min(f32::from(window.viewport_size().width) - 96.)
                        ))
                        .px(px(10.))
                        .py(px(14.))
                        .rounded(px(10.))
                        .bg(colors.color(ColorRole::Floating))
                        .border_1()
                        .border_color(colors.color(ColorRole::Border))
                        .shadow(colors.control_shadows(false))
                        .text_size(px(12.))
                        .text_color(colors.color(ColorRole::Negative))
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .child(error),
                )
            })
            .into_any_element()
    }
    pub fn sidebar_width(&self) -> f32 {
        if self.options.fixture.is_some() && self.options.navigation_fixture == 0 {
            0.
        } else {
            184.
        }
    }
    pub fn accept_navigation(&mut self, notice: Notice, cx: &mut Context<Self>) {
        match notice {
            Notice::Common(locations) => {
                if self.options.fixture.is_none()
                    && let Err(error) =
                        self.navigation
                            .store
                            .send(crate::quick_access::Action::Seed(
                                locations
                                    .iter()
                                    .map(|l| (l.title.clone(), l.path.clone()))
                                    .collect(),
                            ))
                {
                    self.navigation_error(error, cx);
                }
                self.navigation.common = if self.options.navigation_fixture > 0 {
                    (0..self.options.navigation_fixture)
                        .map(|i| Location {
                            title: format!("导航样本 {i:03}"),
                            path: std::path::PathBuf::from(format!(
                                r"V:\Backleaf-fixture\Quick-{i:03}"
                            )),
                            volume: false,
                            capacity: None,
                            entry: None,
                        })
                        .collect()
                } else {
                    locations
                };
            }
            Notice::VolumeSnapshot { epoch, result }
                if epoch
                    == self
                        .navigation
                        .volume_worker
                        .as_ref()
                        .map_or(0, |w| w.epoch()) =>
            {
                match result {
                    Ok(rows) => self.navigation.volumes = rows,
                    Err(error) => self.navigation_error(error, cx),
                }
            }
            Notice::Capacity {
                epoch,
                path,
                title,
                value,
            } if epoch
                == self
                    .navigation
                    .volume_worker
                    .as_ref()
                    .map_or(0, |w| w.epoch()) =>
            {
                if let Some(row) = self
                    .navigation
                    .volumes
                    .iter_mut()
                    .find(|row| row.path == path)
                {
                    row.capacity = value;
                    if let Some(title) = title {
                        row.title = title;
                    }
                }
            }
            Notice::Clipboard(Err(error)) => self.navigation_error(error, cx),
            Notice::Clipboard(Ok(())) => (),
            Notice::Tree {
                token,
                rows,
                truncated,
                error,
            } if token == self.navigation.token => {
                self.navigation.current = rows.iter().position(|r| r.current).unwrap_or(0);
                self.navigation.rows = rows;
                self.navigation.truncated = truncated;
                self.navigation.error = error;
            }
            _ => (),
        }
        cx.notify();
    }
    pub fn close_path_tree(&mut self) {
        self.navigation.open = false;
        self.navigation.pinned = false;
    }
    pub fn show_path_tree(
        &mut self,
        pinned: bool,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.defer_path_tree(false, cx) {
            return;
        }
        if self.options.fixture.is_some()
            || self.files.editor.is_some()
            || self.files.close_guard
            || self.search.open
        {
            return;
        }
        self.navigation.open = true;
        self.navigation.menu = None;
        self.navigation.pinned |= pinned;
        if self.navigation.rows.is_empty() {
            self.navigation.token += 1;
            self.navigation
                .worker
                .tree(self.navigation.token, self.path.clone());
        }
        if pinned && let Some(window) = window {
            window.focus(&self.navigation.focus, cx);
        }
        cx.notify();
    }
    pub fn navigation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let k = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if self.navigation.menu.is_some() {
            return true;
        }
        if !self.navigation.open {
            return false;
        }
        if k == "escape" && !m.control && !m.alt {
            self.close_path_tree();
            window.focus(&self.focus, cx);
            cx.notify();
            return true;
        }
        if !self.navigation.pinned {
            return false;
        }
        if m.control || m.alt || m.platform {
            return false;
        }
        match k {
            "up" => self.navigation.current = self.navigation.current.saturating_sub(1),
            "down" => {
                self.navigation.current =
                    (self.navigation.current + 1).min(self.navigation.rows.len().saturating_sub(1))
            }
            "home" => self.navigation.current = 0,
            "end" => self.navigation.current = self.navigation.rows.len().saturating_sub(1),
            "enter" | "space" => {
                if let Some(row) = self.navigation.rows.get(self.navigation.current) {
                    let path = row.path.clone();
                    self.close_path_tree();
                    self.navigate(path, true, cx);
                    window.focus(&self.focus, cx);
                }
            }
            "tab" => {
                self.close_path_tree();
                window.focus(&self.focus, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }
    pub fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let scrollbar = self.scrollbar(app_scroll::Kind::Sidebar, window, cx);
        div()
            .id("quick-access-sidebar")
            .relative()
            .min_h_0()
            .overflow_hidden()
            .w(px(self.sidebar_width()))
            .flex_none()
            .h_full()
            .px(px(8.))
            .py(px(layout::SCROLL.sidebar_padding_y))
            .bg(colors.color(ColorRole::Chrome))
            .role(Role::Navigation)
            .aria_label(crate::i18n::tr("快捷访问"))
            .child(
                div()
                    .id("quick-access-scroll")
                    .h_full()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.navigation.scroll)
                    .children(
                        self.navigation
                            .common
                            .iter()
                            .chain(self.navigation.volumes.iter())
                            .enumerate()
                            .map(|(i, location)| {
                                let path = location.path.clone();
                                let volume = location.volume;
                                let view = location.entry.as_ref().and_then(|e| {
                                    if let crate::quick_access::Destination::View(v) =
                                        &e.destination
                                    {
                                        Some((e.clone(), v.clone()))
                                    } else {
                                        None
                                    }
                                });
                                let keyboard_view = view.clone();
                                let drag_entry = location.entry.clone();
                                let menu_entry = location.entry.clone();
                                let key_entry = location.entry.clone();
                                let selected = view.as_ref().is_some_and(|(e, _)| {
                                    self.search.saved.as_ref().is_some_and(|s| s.id == e.id)
                                        && self.search.open
                                }) || (view.is_none() && path == self.path);
                                let label = location.title.clone();
                                let capacity_label = location
                                    .capacity
                                    .map_or("容量暂不可用".into(), |v| v.description());
                                div()
                                    .id(SharedString::from(
                                        location
                                            .entry
                                            .as_ref()
                                            .map(|e| format!("quick-{}", e.id))
                                            .unwrap_or_else(|| {
                                                format!(
                                                    "volume-{}",
                                                    navigation::display_path(&path)
                                                )
                                            }),
                                    ))
                                    .w_full()
                                    .h(px(if volume {
                                        layout::SCROLL.volume_row_height
                                    } else {
                                        layout::SCROLL.sidebar_row_height
                                    }))
                                    .when(
                                        i + 1
                                            < self.navigation.common.len()
                                                + self.navigation.volumes.len(),
                                        |d| d.mb(px(layout::SCROLL.sidebar_row_gap)),
                                    )
                                    .flex_none()
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(transparent_black())
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .role(Role::Button)
                                    .aria_label(format!(
                                        "{label}，{}{}",
                                        navigation::display_path(&path),
                                        if volume {
                                            format!("，{capacity_label}")
                                        } else {
                                            String::new()
                                        }
                                    ))
                                    .when(volume, |d| {
                                        d.tooltip(tooltip(capacity_label.clone(), colors))
                                    })
                                    .aria_selected(selected)
                                    .focusable()
                                    .tab_stop(true)
                                    .when(selected, |d| {
                                        // Navigation is a persistent location, not a file
                                        // selection: use the low-emphasis neutral surface so
                                        // the sidebar does not compete with the blue file
                                        // selection accent or the chrome background.
                                        d.bg(colors.color(ColorRole::InactiveSelected))
                                            .text_color(colors.color(ColorRole::OnInactiveSelected))
                                    })
                                    .hover(move |d| {
                                        d.bg(colors.color(if selected {
                                            ColorRole::InactiveSelected
                                        } else {
                                            ColorRole::Floating
                                        }))
                                        .shadow(colors.control_shadows(false))
                                    })
                                    .focus_visible(move |d| {
                                        d.bg(colors.color(if selected {
                                            ColorRole::InactiveSelected
                                        } else {
                                            ColorRole::Floating
                                        }))
                                        .text_color(colors.color(if selected {
                                            ColorRole::OnInactiveSelected
                                        } else {
                                            ColorRole::Primary
                                        }))
                                        .border_color(colors.color(ColorRole::Border))
                                        .shadow(colors.control_shadows(true))
                                    })
                                    .on_click(cx.listener({
                                        let path = path.clone();
                                        move |this, _, window, cx| {
                                            if this.navigation.suppress_click {
                                                this.navigation.suppress_click = false;
                                                return;
                                            }
                                            this.close_path_tree();
                                            if let Some((entry, view)) = &view {
                                                this.open_saved_view(
                                                    entry.clone(),
                                                    view.clone(),
                                                    window,
                                                    cx,
                                                );
                                            } else if volume {
                                                this.request_location(path.clone(), cx);
                                            } else {
                                                this.navigate(path.clone(), true, cx);
                                            }
                                            if !this.rename_editing() {
                                                window.focus(&this.focus, cx);
                                            }
                                        }
                                    }))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, event, _, _| {
                                            if let Some(entry) = &drag_entry {
                                                this.begin_quick_drag(entry.clone(), event);
                                            }
                                        }),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(
                                            move |this, event: &MouseDownEvent, window, cx| {
                                                if let Some(entry) = &menu_entry {
                                                    this.show_quick_menu(
                                                        entry.clone(),
                                                        event.position,
                                                        false,
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            },
                                        ),
                                    )
                                    .on_key_down(cx.listener(
                                        move |this, event: &KeyDownEvent, window, cx| {
                                            if let Some(entry) = &key_entry
                                                && this.quick_key(entry, event, window, cx)
                                            {
                                                cx.stop_propagation();
                                                return;
                                            }
                                            if paint::activation_key(event) {
                                                this.close_path_tree();
                                                if let Some((entry, view)) = &keyboard_view {
                                                    this.open_saved_view(
                                                        entry.clone(),
                                                        view.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                } else if volume {
                                                    this.request_location(path.clone(), cx);
                                                } else {
                                                    this.navigate(path.clone(), true, cx);
                                                }
                                                if !this.rename_editing() {
                                                    window.focus(&this.focus, cx);
                                                }
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .child(
                                        if location.entry.as_ref().is_some_and(|e| {
                                            matches!(
                                                e.destination,
                                                crate::quick_access::Destination::View(_)
                                            )
                                        }) {
                                            icon("search", colors)
                                                .size(px(18.))
                                                .flex_none()
                                                .into_any_element()
                                        } else if location.volume {
                                            icon("drive", colors)
                                                .size(px(18.))
                                                .flex_none()
                                                .into_any_element()
                                        } else {
                                            img(crate::file_icons::FileIcon::Folder
                                                .asset(colors.dark))
                                            .size(px(18.))
                                            .flex_none()
                                            .into_any_element()
                                        },
                                    )
                                    .child(if volume {
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .h(px(20.))
                                                    .line_height(px(20.))
                                                    .truncate()
                                                    .child(label),
                                            )
                                            .child(capacity_meter(location.capacity, colors))
                                            .into_any_element()
                                    } else {
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(label)
                                            .into_any_element()
                                    })
                            }),
                    ),
            )
            .children(scrollbar)
            .into_any_element()
    }
    pub fn path_available(&self, window: &Window, cx: &App) -> f32 {
        let search = if self.options.fixture.is_none() {
            self.search_width(window, cx)
        } else {
            0.
        };
        let operations = if self.files.hub.views().is_empty() {
            0.
        } else {
            72.
        };
        (f32::from(window.viewport_size().width)
            - 264.
            - (layout::CHROME.caption_width + layout::CHROME.caption_gap)
            - search
            - operations)
            .max(80.)
    }
    pub fn path_control(
        &self,
        title: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors;
        let path = navigation::display_path(&self.path);
        if self.options.fixture.is_some() {
            return div()
                .min_w_0()
                .max_w(px(600.))
                .px(px(8.))
                .h(px(32.))
                .flex()
                .items_center()
                .child(div().truncate().child(title))
                .into_any_element();
        }
        let available = self.path_available(window, cx);
        let measure = |text: &str| {
            let shared: SharedString = text.to_owned().into();
            f32::from(
                window
                    .text_system()
                    .shape_line(
                        shared.clone(),
                        px(13.),
                        &[TextRun {
                            len: shared.len(),
                            font: font("Microsoft YaHei UI"),
                            color: colors.color(ColorRole::Primary),
                            ..Default::default()
                        }],
                        None,
                    )
                    .width(),
            )
        };
        let parts = if available < 140. {
            vec![Some(self.path.clone())]
        } else {
            crate::navigation::breadcrumbs(&self.path, available, measure)
        };
        let total = parts.len();
        let fixed = parts
            .iter()
            .take(total.saturating_sub(1))
            .map(|p| {
                p.as_ref()
                    .map(|p| (measure(&crate::navigation::display_name(p)) + 8.).min(120.))
                    .unwrap_or(24.)
            })
            .sum::<f32>()
            + (total.saturating_sub(1) * 12) as f32
            + 32.;
        let current_width = (available - fixed).max(24.);
        div()
            .id("path")
            .min_w_0()
            .max_w(px(available))
            .h(px(layout::CHROME.path_height))
            .px(px(4.))
            .py(px(4.))
            .bg(colors.color(ColorRole::InputRest))
            .flex()
            .items_center()
            .rounded(px(10.))
            .role(Role::Navigation)
            .aria_label(format!("当前位置 {path}"))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    this.close_path_tree();
                    this.edit_path(window, cx);
                }
                cx.stop_propagation();
            }))
            .children(parts.into_iter().enumerate().flat_map(|(i, part)| {
                let current = i == total - 1;
                let mut elements = Vec::new();
                if i > 0 {
                    elements.push(
                        div()
                            .w(px(12.))
                            .flex_none()
                            .h(px(layout::CHROME.crumb_height))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(colors.color(ColorRole::Secondary))
                            .child("›")
                            .into_any_element(),
                    );
                }
                if let Some(path) = part {
                    let label = crate::navigation::display_name(&path);
                    let full = navigation::display_path(&path);
                    if current {
                        elements.push(
                            div()
                                .id("current-directory-crumb")
                                .max_w(px(current_width))
                                .min_w_0()
                                .h(px(layout::CHROME.crumb_height))
                                .px(px(4.))
                                .flex()
                                .items_center()
                                .rounded(px(8.))
                                .focusable()
                                .tab_stop(true)
                                .hover(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(false))
                                })
                                .focus(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(true))
                                })
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener({
                                        let path = path.clone();
                                        move |this, event: &MouseDownEvent, window, cx| {
                                            this.show_crumb_menu(
                                                path.clone(),
                                                event.position,
                                                false,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.edit_path(window, cx);
                                    cx.stop_propagation();
                                }))
                                .on_key_down(cx.listener({
                                    let path = path.clone();
                                    move |this, event: &KeyDownEvent, window, cx| {
                                        if path_menu_key(event) {
                                            this.show_crumb_menu(
                                                path.clone(),
                                                point(px(88.), px(layout::CHROME.bar_height)),
                                                true,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                            return;
                                        }
                                        if paint::activation_key(event) {
                                            this.edit_path(window, cx);
                                            cx.stop_propagation();
                                        }
                                    }
                                }))
                                .role(Role::Button)
                                .aria_label(format!("编辑当前位置 {full}"))
                                .aria_value(full.clone())
                                .text_color(colors.color(ColorRole::Primary))
                                .child(div().truncate().child(label))
                                .into_any_element(),
                        );
                    } else {
                        elements.push(
                            div()
                                .id(("ancestor-crumb", i))
                                .h(px(layout::CHROME.crumb_height))
                                .max_w(px(120.))
                                .min_w_0()
                                .px(px(4.))
                                .rounded(px(8.))
                                .flex()
                                .items_center()
                                .role(Role::Link)
                                .aria_label(format!("前往 {full}"))
                                .focusable()
                                .tab_stop(true)
                                .text_color(colors.color(ColorRole::Secondary))
                                .hover(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(false))
                                        .text_color(colors.color(ColorRole::Primary))
                                })
                                .focus(move |d| {
                                    d.bg(colors.color(ColorRole::Floating))
                                        .shadow(colors.control_shadows(true))
                                })
                                .on_click(cx.listener({
                                    let path = path.clone();
                                    move |this, _, window, cx| {
                                        this.close_path_tree();
                                        this.navigate(path.clone(), true, cx);
                                        if !this.rename_editing() {
                                            window.focus(&this.focus, cx);
                                        }
                                        cx.stop_propagation();
                                    }
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener({
                                        let path = path.clone();
                                        move |this, event: &MouseDownEvent, window, cx| {
                                            this.show_crumb_menu(
                                                path.clone(),
                                                event.position,
                                                false,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                )
                                .on_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, window, cx| {
                                        if path_menu_key(event) {
                                            this.show_crumb_menu(
                                                path.clone(),
                                                point(px(88.), px(layout::CHROME.bar_height)),
                                                true,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                            return;
                                        }
                                        if paint::activation_key(event) {
                                            this.close_path_tree();
                                            this.navigate(path.clone(), true, cx);
                                            if !this.rename_editing() {
                                                window.focus(&this.focus, cx);
                                            }
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .child(div().truncate().child(label))
                                .into_any_element(),
                        );
                    }
                } else {
                    elements.push(
                        div()
                            .id("crumb-overflow")
                            .w(px(24.))
                            .h(px(layout::CHROME.crumb_height))
                            .flex_none()
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::Button)
                            .aria_label(crate::i18n::tr("显示折叠的上级路径"))
                            .aria_expanded(self.navigation.open)
                            .focusable()
                            .tab_stop(true)
                            .hover(move |d| {
                                d.bg(colors.color(ColorRole::Floating))
                                    .shadow(colors.control_shadows(false))
                            })
                            .focus(move |d| {
                                d.bg(colors.color(ColorRole::Floating))
                                    .shadow(colors.control_shadows(true))
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_ancestor_menu(window, cx);
                                cx.stop_propagation();
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                if paint::activation_key(event) {
                                    this.show_ancestor_menu(window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .child("…")
                            .into_any_element(),
                    );
                }
                elements
            }))
            .child(
                div()
                    .id("path-dropdown-arrow")
                    .w(px(24.))
                    .h(px(layout::CHROME.crumb_height))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .role(Role::Button)
                    .aria_label(crate::i18n::tr("浏览当前目录层级"))
                    .aria_expanded(self.navigation.open)
                    .focusable()
                    .tab_stop(true)
                    .hover(move |d| {
                        d.bg(colors.color(ColorRole::Floating))
                            .shadow(colors.control_shadows(false))
                    })
                    .focus(move |d| {
                        d.bg(colors.color(ColorRole::InputActive))
                            .shadow(colors.input_shadows())
                    })
                    // Keep the dropdown toggle's own press handling instead of
                    // letting the title bar dismiss handler close the tree that
                    // this click is about to toggle.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.navigation.open {
                            this.close_path_tree();
                            cx.notify();
                        } else {
                            this.navigation.rows.clear();
                            this.show_path_tree(true, Some(window), cx);
                        }
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if paint::activation_key(event) {
                            if this.navigation.open {
                                this.close_path_tree();
                                cx.notify();
                            } else {
                                this.navigation.rows.clear();
                                this.show_path_tree(true, Some(window), cx);
                            }
                            cx.stop_propagation();
                        }
                    }))
                    .child(icon("more", colors)),
            )
            .into_any_element()
    }
    pub fn show_ancestor_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.defer_path_tree(true, cx) {
            return;
        }
        self.navigation.token += 1;
        self.navigation.menu = None;
        let mut bytes = 0;
        let mut rows = Vec::new();
        for path in self.path.ancestors().skip(1).take(80) {
            bytes += path.as_os_str().len() * 2;
            if bytes > 256 * 1024 {
                break;
            }
            rows.push(TreeRow {
                title: crate::navigation::display_name(path),
                path: path.to_path_buf(),
                depth: 0,
                current: false,
            });
        }
        rows.reverse();
        self.navigation.rows = rows;
        self.navigation.open = true;
        self.navigation.pinned = true;
        self.navigation.current = 0;
        self.navigation.error = None;
        self.navigation.truncated = false;
        window.focus(&self.navigation.focus, cx);
        cx.notify();
    }
    pub fn path_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        div()
            .id("path-tree-navigation")
            .absolute()
            .left(px(80.))
            .top(px(layout::CHROME.bar_height))
            .w(px(360.))
            .max_w(px((f32::from(window.viewport_size().width) - 96.).max(100.)))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 60.).min(480.)
            ))
            .px(px(4.))
            .py(px(8.))
            .rounded(px(12.))
            .bg(colors.color(ColorRole::Floating))
            .border_1()
            .border_color(colors.color(ColorRole::Border))
            .shadow(vec![colors.shadow()])
            .occlude()
            .track_focus(&self.navigation.focus)
            .role(Role::Tree)
            .aria_label(crate::i18n::tr("当前位置导航"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left
                    && f32::from(event.position.y) > layout::CHROME.bar_height
                {
                    this.close_path_tree();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("path-tree-scroll")
                    .max_h(px(
                        (f32::from(window.viewport_size().height) - 80.).min(440.)
                    ))
                    .overflow_y_scroll()
                    .children(self.navigation.rows.iter().enumerate().map(|(i, row)| {
                        let path = row.path.clone();
                        let focus = self.navigation.pinned && self.navigation.current == i;
                        div()
                            .id(("path-tree-row", i))
                            .w_full()
                            .h(px(32.))
                            .pl(px(8. + row.depth.min(5) as f32 * 12.))
                            .pr(px(8.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .role(Role::TreeItem)
                            .aria_label(navigation::display_path(&path))
                            .aria_selected(row.current)
                            .when(focus, |d| d.aria_active_descendant())
                            .when(row.current || focus, |d| {
                                d.bg(colors.color(ColorRole::Hover))
                            })
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_path_tree();
                                this.navigate(path.clone(), true, cx);
                                if !this.rename_editing() {
                                    window.focus(&this.focus, cx);
                                }
                                cx.stop_propagation();
                            }))
                            .child(div().truncate().child(row.title.clone()))
                    }))
                    .when(
                        self.navigation.rows.is_empty() && self.navigation.error.is_none(),
                        |d| {
                            d.child(
                                div()
                                    .px(px(8.))
                                    .py(px(16.))
                                    .text_color(colors.color(ColorRole::Secondary))
                                    .child(crate::i18n::tr("正在读取…")),
                            )
                        },
                    )
                    .when_some(self.navigation.error.clone(), |d, error| {
                        d.child(
                            div()
                                .px(px(8.))
                                .py(px(16.))
                                .text_size(px(12.))
                                .text_color(colors.color(ColorRole::Negative))
                                .child(error),
                        )
                    })
                    .when(self.navigation.truncated, |d| {
                        d.child(
                            div()
                                .px(px(8.))
                                .py(px(8.))
                                .text_size(px(12.))
                                .text_color(colors.color(ColorRole::Secondary))
                                .child(crate::i18n::tr("更多文件夹可在当前位置浏览")),
                        )
                    }),
            )
            .into_any_element()
    }
}
fn path_menu_key(event: &KeyDownEvent) -> bool {
    let k = &event.keystroke;
    !k.modifiers.control
        && !k.modifiers.alt
        && !k.modifiers.platform
        && (k.key == "menu" || (k.key == "f10" && k.modifiers.shift))
}
