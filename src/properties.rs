//! Independent properties windows never borrow the owning file-browser entity.
use crate::{
    file_icons::FileIcon,
    model::Detail,
    navigation, paint,
    properties_capacity::{Scan, Snapshot},
    selectable::Selectable,
    theme::{self, Palette, Role as ColorRole},
    volume_capacity,
};
use gpui::{prelude::*, *};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

#[derive(Default)]
struct Registry(HashMap<(PathBuf, Option<[u8; 33]>), WindowHandle<Properties>>);
impl Global for Registry {}

pub fn open(
    detail: Detail,
    colors: &'static Palette,
    reduced_motion: bool,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Properties>> {
    let live = cx.windows();
    cx.default_global::<Registry>()
        .0
        .retain(|_, handle| live.contains(&(*handle).into()));
    let key = (detail.path.clone(), detail.identity);
    if let Some(handle) = cx.global::<Registry>().0.get(&key).copied() {
        handle.update(cx, |_, window, _| window.activate_window())?;
        return Ok(handle);
    }
    let title = crate::trf!("{} · 详情", detail.name);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(440.), px(544.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_min_size: Some(size(px(340.), px(360.))),
            ..Default::default()
        },
        move |window, cx| cx.new(|cx| Properties::new(detail, colors, reduced_motion, window, cx)),
    )?;
    cx.default_global::<Registry>().0.insert(key, handle);
    Ok(handle)
}
pub struct Properties {
    detail: Detail,
    colors: &'static Palette,
    reduced_motion: bool,
    name: Entity<Selectable>,
    path: Entity<Selectable>,
    focus: FocusHandle,
    scan: Scan,
    pub(crate) snapshot: Snapshot,
    counter: Counter,
    poll: Option<Task<()>>,
    scroll: ScrollHandle,
    scroll_drag: Option<(f32, crate::scrollbar::Geometry)>,
    scroll_hover: bool,
    _appearance: Subscription,
    _bounds: Subscription,
}
impl Properties {
    fn new(
        detail: Detail,
        colors: &'static Palette,
        reduced_motion: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::configure_native_corners(window, colors.dark);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        window.activate_window();
        let name = cx.new(|cx| Selectable::new(detail.name.clone(), colors, cx));
        let path = cx.new(|cx| Selectable::new(navigation::display_path(&detail.path), colors, cx));
        let scan = Self::scanner(&detail);
        let initial_bytes = detail.bytes.map(u128::from).unwrap_or(0);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            let colors = theme::for_window(window);
            this.colors = colors;
            crate::configure_native_corners(window, colors.dark);
            for text in [&this.name, &this.path] {
                text.update(cx, |text, cx| {
                    text.colors = colors;
                    cx.notify();
                });
            }
            cx.notify();
        });
        let bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.scroll_drag = None;
            Self::measure_after_frame(window, cx);
            cx.notify();
        });
        Self::measure_after_frame(window, cx);
        let mut this = Self {
            detail,
            colors,
            reduced_motion,
            name,
            path,
            focus,
            scan,
            snapshot: Snapshot::default(),
            counter: Counter::new(initial_bytes),
            poll: None,
            scroll: ScrollHandle::new(),
            scroll_drag: None,
            scroll_hover: false,
            _appearance: appearance,
            _bounds: bounds,
        };
        this.start_poll(cx);
        this
    }
    fn scanner(detail: &Detail) -> Scan {
        Scan::new(
            detail.path.clone(),
            detail
                .identity
                .as_ref()
                .and_then(|value| crate::operations::windows::identity::Identity::decode(value)),
        )
    }
    fn measure_after_frame(window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = view.update(cx, |_, cx| cx.notify());
        });
    }
    pub(crate) fn scroll_geometry(&self) -> Option<crate::scrollbar::Geometry> {
        let viewport = f64::from(f32::from(self.scroll.bounds().size.height))
            - 2. * f64::from(crate::layout::SCROLL.inset_y);
        let maximum = f64::from(f32::from(self.scroll.max_offset().y));
        crate::scrollbar::Geometry::new(
            viewport,
            viewport + maximum,
            -f64::from(f32::from(self.scroll.offset().y)),
            crate::layout::SCROLL.minimum_thumb_height,
        )
    }
    fn set_scroll(&self, value: f64, cx: &mut Context<Self>) {
        if let Some(g) = self.scroll_geometry() {
            self.scroll
                .set_offset(point(px(0.), px(-value.clamp(0., g.maximum) as f32)));
            cx.notify();
        }
    }
    fn scrollbar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let g = self.scroll_geometry()?;
        let s = &crate::layout::SCROLL;
        let colors = self.colors;
        let active = self.scroll_hover || self.scroll_drag.is_some();
        let owner = cx.entity().downgrade();
        let width = if active {
            s.active_thumb_width
        } else {
            s.thumb_width
        };
        Some(
            div()
                .id("properties-scrollbar")
                .absolute()
                .right(px(s.rail_inset))
                .top(px(44. + s.inset_y))
                .bottom(px(s.inset_y))
                .w(px(s.rail_width))
                .occlude()
                .role(Role::ScrollBar)
                .aria_label(crate::i18n::tr("详情滚动条"))
                .aria_orientation(accesskit::Orientation::Vertical)
                .aria_min_numeric_value(0.)
                .aria_max_numeric_value(100.)
                .aria_numeric_value(g.offset / g.maximum * 100.)
                .focusable()
                .tab_stop(true)
                .rounded(px(6.))
                .focus_visible(move |d| d.bg(colors.color(ColorRole::Hover)))
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    this.scroll_hover = *hovered;
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        let pointer = f32::from(event.position.y);
                        let local = pointer - 44. - s.inset_y;
                        if local >= g.top && local <= g.top + g.thumb {
                            this.scroll_drag = Some((pointer, g));
                        } else {
                            this.set_scroll(
                                g.offset
                                    + if local < g.top {
                                        -g.viewport
                                    } else {
                                        g.viewport
                                    },
                                cx,
                            );
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                    if let Some(g) = this.scroll_geometry() {
                        let delta =
                            f64::from(f32::from(event.delta.pixel_delta(window.line_height()).y));
                        this.set_scroll(g.offset - delta, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    let Some(g) = this.scroll_geometry() else {
                        return;
                    };
                    let target = match event.keystroke.key.as_str() {
                        "up" => g.offset - 32.,
                        "down" => g.offset + 32.,
                        "pageup" => g.offset - g.viewport,
                        "pagedown" => g.offset + g.viewport,
                        "home" => 0.,
                        "end" => g.maximum,
                        _ => return,
                    };
                    this.set_scroll(target, cx);
                    cx.stop_propagation();
                }))
                .on_a11y_action(AccessibleAction::SetValue, move |data, _, cx| {
                    if let Some(accesskit::ActionData::NumericValue(value)) = data
                        && value.is_finite()
                    {
                        let _ = owner.update(cx, |this, cx| {
                            if let Some(g) = this.scroll_geometry() {
                                this.set_scroll(g.maximum * value.clamp(0., 100.) / 100., cx);
                            }
                        });
                    }
                })
                .child(
                    div()
                        .absolute()
                        .top(px(g.top))
                        .left(px((s.rail_width - width) / 2.))
                        .w(px(width))
                        .h(px(g.thumb))
                        .rounded(px(width / 2.))
                        .bg(colors.color(ColorRole::Secondary).opacity(if active {
                            s.active_thumb_opacity
                        } else {
                            s.thumb_opacity
                        })),
                )
                .into_any_element(),
        )
    }
    fn start_poll(&mut self, cx: &mut Context<Self>) {
        self.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let keep_going = this
                    .update(cx, |this, cx| {
                        let next = this.scan.snapshot();
                        if next.version != this.snapshot.version {
                            if next.initialized {
                                this.counter.set(next.bytes, Instant::now());
                            }
                            this.snapshot = next;
                            cx.notify();
                        }
                        !this.snapshot.done
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
            }
        }));
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.poll = None;
        self.scan = Self::scanner(&self.detail);
        self.snapshot = Snapshot::default();
        self.counter = Counter::new(0);
        self.start_poll(cx);
        cx.notify();
    }
    #[cfg(test)]
    pub(crate) fn cancellation_probe(&self) -> Box<dyn Fn() -> bool> {
        self.scan.cancellation_probe()
    }
    #[cfg(test)]
    pub(crate) fn displayed_path(&self) -> String {
        navigation::display_path(&self.detail.path)
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        action: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let colors = self.colors;
        let action = std::rc::Rc::new(action);
        let keyboard = action.clone();
        div()
            .id(id)
            .size(px(32.))
            .flex_none()
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .role(Role::Button)
            .aria_label(label)
            .tooltip(paint::tooltip(label.into(), colors))
            .focusable()
            .tab_stop(true)
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(false))
            })
            .focus_visible(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(true))
            })
            .on_click(move |event, window, cx| action(event, window, cx))
            .on_key_down(move |event, window, cx| {
                if paint::activation_key(event) {
                    keyboard(&ClickEvent::default(), window, cx);
                    cx.stop_propagation();
                }
            })
            .child(paint::icon(id, colors))
            .into_any_element()
    }
    fn date(&self, label: &'static str, value: Option<SystemTime>) -> AnyElement {
        let value = value
            .map(|time| {
                chrono::DateTime::<chrono::Local>::from(time)
                    .format("%Y/%m/%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into());
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(self.colors.color(ColorRole::Secondary))
                    .child(label),
            )
            .child(value)
            .into_any_element()
    }
}
impl Render for Properties {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let now = Instant::now();
        let reduce = self.reduced_motion
            || cx.reduce_motion()
            || !window.is_window_active()
            || !window.is_visible();
        let displayed = self.counter.value(now, reduce);
        if !reduce && self.counter.animating(now) {
            window.request_animation_frame();
        }
        let known = self.snapshot.initialized
            && ((self.snapshot.error.is_none()
                && self.snapshot.skipped == 0
                && self.snapshot.unavailable == 0)
                || self.snapshot.files > 0
                || self.snapshot.folders > 0);
        let bytes = if known {
            volume_capacity::bytes(displayed)
        } else {
            "—".into()
        };
        let counts = if self.detail.directory {
            format!(
                "{} 个文件 · {} 个文件夹",
                grouped(self.snapshot.files as u128),
                grouped(self.snapshot.folders as u128)
            )
        } else {
            crate::trf!("{} 字节", grouped(self.snapshot.bytes))
        };
        let summary = if let Some(error) = &self.snapshot.error {
            error.clone()
        } else if !self.snapshot.running && !self.snapshot.done {
            crate::i18n::tr_owned("等待统计")
        } else if !self.snapshot.initialized {
            crate::i18n::tr_owned("正在统计")
        } else if !self.snapshot.done {
            crate::trf!("正在统计 · {}", counts)
        } else {
            counts
        };
        let incomplete = self.snapshot.unavailable > 0 || self.snapshot.skipped > 0;
        let size_tip = if known {
            format!(
                "{} 字节 · 逻辑大小，不展开链接或离线项",
                grouped(self.snapshot.bytes)
            )
        } else {
            crate::i18n::tr_owned("尚未获得大小")
        };
        let warning = crate::trf!(
            "{} 项无法读取 · {} 项未展开",
            grouped(self.snapshot.unavailable as u128),
            grouped(self.snapshot.skipped as u128)
        );
        let kind = if self.detail.directory {
            crate::i18n::tr_owned("文件夹")
        } else {
            self.detail
                .path
                .extension()
                .filter(|ext| !ext.is_empty())
                .map(|ext| format!("{} 文件", ext.to_string_lossy().to_uppercase()))
                .unwrap_or_else(|| crate::i18n::tr_owned("文件"))
        };
        let art = FileIcon::for_name(
            self.detail.path.file_name().unwrap_or_default(),
            self.detail.directory,
        );
        let name = self.detail.name.clone();
        let path = navigation::display_path(&self.detail.path);
        div()
            .id("properties-window")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.color(ColorRole::Canvas))
            .text_color(colors.color(ColorRole::Primary))
            .font_family("Segoe UI")
            .text_size(px(13.))
            .line_height(px(21.))
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some((pointer, g)) = this.scroll_drag {
                    if !event.dragging() {
                        this.scroll_drag = None;
                    } else {
                        this.set_scroll(
                            g.dragged_offset(f32::from(event.position.y) - pointer),
                            cx,
                        );
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.scroll_drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.scroll_drag = None;
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                if key == "escape" && this.scroll_drag.take().is_some() {
                    cx.stop_propagation();
                    cx.notify();
                } else if key == "escape" || (modifiers.control && key == "w") {
                    window.remove_window();
                    cx.stop_propagation();
                } else if key == "f5" {
                    this.refresh(cx);
                    cx.stop_propagation();
                } else if key == "tab" && !modifiers.control && !modifiers.alt {
                    if modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .h(px(44.))
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .px(px(4.))
                            .window_control_area(WindowControlArea::Drag)
                            .child(crate::i18n::tr("详情")),
                    )
                    .child(self.button("close", "关闭详情", |_, window, _| {
                        window.remove_window()
                    })),
            )
            .child(
                div()
                    .id("properties-scroll")
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .flex_1()
                    .min_h_0()
                    .px(px(12.))
                    .pr(px(12. + crate::layout::SCROLL.gutter))
                    .py(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .id("properties-identity")
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(paint::file_artwork(
                                art.asset(colors.dark).into(),
                                44.,
                                art != FileIcon::Folder,
                                colors,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(self.name.clone())
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(colors.color(ColorRole::Secondary))
                                            .child(kind),
                                    ),
                            )
                            .child(self.button("copy", "复制完整名称", move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                            })),
                    )
                    .child(
                        div()
                            .id("properties-size")
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(colors.color(ColorRole::Secondary))
                                    .child(crate::i18n::tr("大小")),
                            )
                            .child(
                                div()
                                    .id("properties-size-value")
                                    .h(px(32.))
                                    .text_size(px(24.))
                                    .line_height(px(32.))
                                    .role(Role::Label)
                                    .aria_label(crate::i18n::tr("逻辑大小"))
                                    .aria_value(bytes.clone())
                                    .tooltip(paint::tooltip(size_tip, colors))
                                    .child(bytes),
                            )
                            .child(
                                div()
                                    .id("properties-size-summary")
                                    .h(px(18.))
                                    .truncate()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(colors.color(if self.snapshot.error.is_some() {
                                        ColorRole::Negative
                                    } else {
                                        ColorRole::Secondary
                                    }))
                                    .tooltip(paint::tooltip(summary.clone(), colors))
                                    .child(summary),
                            )
                            .when(incomplete, |d| {
                                d.child(
                                    div()
                                        .id("properties-incomplete")
                                        .text_size(px(12.))
                                        .line_height(px(18.))
                                        .text_color(colors.color(ColorRole::Negative))
                                        .tooltip(paint::tooltip(warning, colors))
                                        .child(crate::i18n::tr("部分项目未统计")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("properties-location")
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .line_height(px(18.))
                                            .text_color(colors.color(ColorRole::Secondary))
                                            .child(crate::i18n::tr("位置")),
                                    )
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
                            .child(self.path.clone()),
                    )
                    .child(self.date(crate::i18n::tr("创建时间"), self.snapshot.created))
                    .child(self.date(crate::i18n::tr("修改时间"), self.snapshot.modified)),
            )
            .children(self.scrollbar(cx))
    }
}
struct Counter {
    from: u128,
    target: u128,
    at: Instant,
}
impl Counter {
    fn new(value: u128) -> Self {
        Self {
            from: value,
            target: value,
            at: Instant::now(),
        }
    }
    fn set(&mut self, target: u128, now: Instant) {
        self.from = self.value(now, false);
        self.target = target;
        self.at = now;
    }
    fn animating(&self, now: Instant) -> bool {
        self.from != self.target
            && now.saturating_duration_since(self.at) < Duration::from_millis(120)
    }
    fn value(&self, now: Instant, reduced: bool) -> u128 {
        if reduced || !self.animating(now) {
            return self.target;
        }
        let t = (now.saturating_duration_since(self.at).as_secs_f64() / 0.12).clamp(0., 1.);
        let eased = t * t * (3. - 2. * t);
        if self.target >= self.from {
            self.from
                .saturating_add(((self.target - self.from) as f64 * eased) as u128)
                .min(self.target)
        } else {
            self.from
                .saturating_sub(((self.from - self.target) as f64 * eased) as u128)
                .max(self.target)
        }
    }
}
fn grouped(n: u128) -> String {
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
#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn counter_coalesces_smoothly_but_finishes_exactly_and_honors_reduced_motion() {
        let now = Instant::now();
        let mut c = Counter::new(0);
        c.set(1000, now);
        assert_eq!(c.value(now, false), 0);
        assert_eq!(c.value(now + Duration::from_millis(60), false), 500);
        c.set(2000, now + Duration::from_millis(60));
        assert_eq!(c.value(now + Duration::from_millis(60), false), 500);
        assert_eq!(c.value(now + Duration::from_millis(180), false), 2000);
        c.set(u128::MAX, now + Duration::from_secs(1));
        assert_eq!(c.value(now + Duration::from_secs(1), true), u128::MAX);
        assert_eq!(c.value(now + Duration::from_secs(2), false), u128::MAX);
    }
}
