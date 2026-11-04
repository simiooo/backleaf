use super::*;
use crate::navigation::{Location, Notice, TreeRow, Worker};
pub struct NavigationUi {
    pub worker: Worker,
    pub common: Vec<Location>,
    pub rows: Vec<TreeRow>,
    pub open: bool,
    pub pinned: bool,
    pub over_path: bool,
    pub over_tree: bool,
    pub token: u64,
    pub hover_token: u64,
    pub current: usize,
    pub truncated: bool,
    pub error: Option<String>,
    pub focus: FocusHandle,
}
impl NavigationUi {
    pub fn new(worker: Worker, cx: &mut Context<Backleaf>) -> Self {
        Self {
            worker,
            common: vec![],
            rows: vec![],
            open: false,
            pinned: false,
            over_path: false,
            over_tree: false,
            token: 0,
            hover_token: 0,
            current: 0,
            truncated: false,
            error: None,
            focus: cx.focus_handle(),
        }
    }
}
impl Backleaf {
    pub fn sidebar_width(&self) -> f32 {
        if self.options.fixture.is_some() {
            0.
        } else {
            184.
        }
    }
    pub fn inline_details_width(&self, window: &Window) -> f32 {
        if self.details.is_some() && f32::from(window.viewport_size().width) >= 900. {
            320.
        } else {
            0.
        }
    }
    pub fn accept_navigation(&mut self, notice: Notice, cx: &mut Context<Self>) {
        match notice {
            Notice::Common(locations) => self.navigation.common = locations,
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
        self.navigation.hover_token += 1;
    }
    pub fn show_path_tree(
        &mut self,
        pinned: bool,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.options.fixture.is_some() || self.files.editor.is_some() || self.files.close_guard {
            return;
        }
        self.navigation.open = true;
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
    pub fn path_hover(&mut self, over: bool, tree: bool, cx: &mut Context<Self>) {
        if tree {
            self.navigation.over_tree = over;
        } else {
            self.navigation.over_path = over;
        }
        self.navigation.hover_token += 1;
        let token = self.navigation.hover_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(if over { 180 } else { 260 }))
                .await;
            let _ = this.update(cx, |this, cx| {
                if token != this.navigation.hover_token {
                    return;
                }
                if this.navigation.over_path || this.navigation.over_tree {
                    this.show_path_tree(false, None, cx);
                } else if !this.navigation.pinned {
                    this.close_path_tree();
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub fn navigation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let k = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
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
    pub fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        div()
            .id("quick-access-sidebar")
            .w(px(self.sidebar_width()))
            .flex_none()
            .h_full()
            .px(px(8.))
            .py(px(16.))
            .bg(colors.color(ColorRole::Chrome))
            .role(Role::Navigation)
            .aria_label("快捷访问")
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(colors.color(ColorRole::Secondary))
                    .mb(px(16.))
                    .child("快捷访问"),
            )
            .child(
                div()
                    .id("quick-access-scroll")
                    .h_full()
                    .overflow_y_scroll()
                    .children(
                        self.navigation
                            .common
                            .iter()
                            .enumerate()
                            .map(|(i, location)| {
                                let path = location.path.clone();
                                let selected = path == self.path;
                                let label = location.title.clone();
                                div()
                                    .id(("quick-access", i))
                                    .w_full()
                                    .h(px(32.))
                                    .mb(px(4.))
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .role(Role::Button)
                                    .aria_label(format!("{label}，{}", path.to_string_lossy()))
                                    .aria_selected(selected)
                                    .focusable()
                                    .tab_stop(true)
                                    .tooltip(tooltip(path.to_string_lossy().into_owned(), colors))
                                    .when(selected, |d| {
                                        d.bg(colors.color(ColorRole::Selected))
                                            .text_color(colors.color(ColorRole::OnSelected))
                                    })
                                    .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                                    .focus_visible(move |d| d.shadow(vec![colors.focus_ring()]))
                                    .on_click(cx.listener({
                                        let path = path.clone();
                                        move |this, _, window, cx| {
                                            this.close_path_tree();
                                            this.navigate(path.clone(), true, cx);
                                            window.focus(&this.focus, cx);
                                        }
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, event: &KeyDownEvent, window, cx| {
                                            if paint::activation_key(event) {
                                                this.close_path_tree();
                                                this.navigate(path.clone(), true, cx);
                                                window.focus(&this.focus, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .child(icon("location", colors))
                                    .child(div().flex_1().min_w_0().truncate().child(label))
                            }),
                    ),
            )
            .into_any_element()
    }
    pub fn path_control(
        &self,
        title: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors;
        let path = self.path.to_string_lossy().into_owned();
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
        let available = (f32::from(window.viewport_size().width)
            - 8.
            - 64.
            - 138.
            - 32.
            - if self.files.hub.views().is_empty() {
                0.
            } else {
                64.
            })
        .clamp(120., 600.);
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
        let parts = crate::navigation::breadcrumbs(&self.path, available, measure);
        let total = parts.len();
        let fixed = parts
            .iter()
            .take(total.saturating_sub(1))
            .map(|p| {
                p.as_ref()
                    .map(|p| (measure(&crate::navigation::display_name(p)) + 8.).min(120.))
                    .unwrap_or(32.)
            })
            .sum::<f32>()
            + (total.saturating_sub(1) * 16) as f32
            + 32.;
        let current_width = (available - fixed).max(48.);
        div()
            .id("path")
            .min_w_0()
            .max_w(px(available))
            .h(px(32.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .role(Role::Navigation)
            .aria_label(format!("当前位置 {path}"))
            .when(self.navigation.open, |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(vec![colors.shadow()])
            })
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(vec![colors.shadow()])
            })
            .on_hover(cx.listener(|this, over, _, cx| this.path_hover(*over, false, cx)))
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
                            .w(px(16.))
                            .flex_none()
                            .h(px(32.))
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
                    let full = path.to_string_lossy().into_owned();
                    if current {
                        elements.push(
                            div()
                                .id("current-directory-crumb")
                                .max_w(px(current_width))
                                .min_w_0()
                                .h(px(32.))
                                .px(px(4.))
                                .flex()
                                .items_center()
                                .role(Role::Label)
                                .aria_label(format!("当前位置 {full}"))
                                .aria_value(full.clone())
                                .text_color(colors.color(ColorRole::Primary))
                                .tooltip(tooltip(full, colors))
                                .child(div().truncate().child(label))
                                .into_any_element(),
                        );
                    } else {
                        elements.push(
                            div()
                                .id(("ancestor-crumb", i))
                                .h(px(32.))
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
                                    d.bg(colors.color(ColorRole::Hover))
                                        .text_color(colors.color(ColorRole::Primary))
                                })
                                .focus_visible(move |d| d.shadow(vec![colors.focus_ring()]))
                                .tooltip(tooltip(full, colors))
                                .on_click(cx.listener({
                                    let path = path.clone();
                                    move |this, _, window, cx| {
                                        this.close_path_tree();
                                        this.navigate(path.clone(), true, cx);
                                        window.focus(&this.focus, cx);
                                        cx.stop_propagation();
                                    }
                                }))
                                .on_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, window, cx| {
                                        if paint::activation_key(event) {
                                            this.close_path_tree();
                                            this.navigate(path.clone(), true, cx);
                                            window.focus(&this.focus, cx);
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
                            .w(px(32.))
                            .h(px(32.))
                            .flex_none()
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::Button)
                            .aria_label("显示折叠的上级路径")
                            .aria_expanded(self.navigation.open)
                            .focusable()
                            .tab_stop(true)
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .focus_visible(move |d| d.shadow(vec![colors.focus_ring()]))
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
            .child(self.button(
                "more",
                "浏览当前目录层级",
                cx.listener(|this, _, window, cx| {
                    this.navigation.rows.clear();
                    this.show_path_tree(true, Some(window), cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }
    fn show_ancestor_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation.token += 1;
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
            .top(px(42.))
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
            .aria_label("当前位置导航")
            .on_hover(cx.listener(|this, over, _, cx| this.path_hover(*over, true, cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left && f32::from(event.position.y) > 44. {
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
                            .aria_label(path.to_string_lossy().into_owned())
                            .aria_selected(row.current)
                            .when(focus, |d| d.aria_active_descendant())
                            .when(row.current || focus, |d| {
                                d.bg(colors.color(ColorRole::Hover))
                            })
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .tooltip(tooltip(path.to_string_lossy().into_owned(), colors))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_path_tree();
                                this.navigate(path.clone(), true, cx);
                                window.focus(&this.focus, cx);
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
                                    .child("正在读取…"),
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
                                .child("更多文件夹可在当前位置浏览"),
                        )
                    }),
            )
            .into_any_element()
    }
}
