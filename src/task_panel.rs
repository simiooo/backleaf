use crate::theme::{Palette, Role as ColorRole};
use crate::{
    metrics::Metrics,
    paint::{SharedFlowCache, flowing_name, icon, tooltip},
};
use gpui::{prelude::*, *};
use std::{cell::RefCell, rc::Rc, time::Instant};

pub struct ClosePanel;
pub struct TaskPanel {
    pub colors: &'static Palette,
    pub expanded: bool,
    pub active: usize,
    pub reduced: bool,
    pub started: Instant,
    pub cache: SharedFlowCache,
    pub metrics: Rc<RefCell<Metrics>>,
}
impl EventEmitter<ClosePanel> for TaskPanel {}
impl Render for TaskPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let viewport = window.viewport_size();
        self.metrics.borrow_mut().begin(
            [f32::from(viewport.width), f32::from(viewport.height)],
            window.scale_factor(),
        );
        let phase = (self.started.elapsed().as_secs_f32() / 1.8).fract();
        let animate = window.is_visible()
            && !self.metrics.borrow().session_locked
            && !self.reduced
            && !cx.reduce_motion();
        let marker = self.metrics.clone();
        div()
            .id("task-panel")
            .occlude()
            .on_mouse_down_out(cx.listener(|_, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left {
                    cx.emit(ClosePanel);
                }
            }))
            .w_full()
            .px(px(crate::layout::LAYOUT.panel.padding_x))
            .py(px(crate::layout::LAYOUT.panel.padding_y))
            .bg(colors.color(ColorRole::Floating))
            .text_color(colors.color(ColorRole::Primary))
            .border_1()
            .border_color(colors.color(ColorRole::Border))
            .rounded(px(crate::layout::LAYOUT.panel.radius))
            .shadow(vec![colors.shadow()])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(crate::layout::LAYOUT.control.hit_size))
                    .mb(px(8.))
                    .child(
                        div()
                            .id("task-target")
                            .flex_1()
                            .min_w_0()
                            .h(px(crate::layout::LAYOUT.control.hit_size))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(crate::layout::LAYOUT.control.radius))
                            .role(Role::Button)
                            .aria_label("展开或折叠渲染样本")
                            .focusable()
                            .tab_stop(true)
                            .focus_visible(move |d| d.shadow(vec![colors.focus_ring()]))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.expanded = !this.expanded;
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }))
                            .tooltip(tooltip(
                                "复制 100,000 项 → C:\\渲染基准\\文档（渲染样本）".into(),
                                colors,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.expanded = !this.expanded;
                                cx.notify();
                            }))
                            .child(div().flex_1().truncate().child("文档 · 渲染样本"))
                            .child(icon(
                                if self.expanded { "collapse" } else { "more" },
                                colors,
                            )),
                    )
                    .child(
                        div()
                            .id("task-cancel")
                            .h(px(crate::layout::LAYOUT.control.hit_size))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .rounded(px(crate::layout::LAYOUT.control.radius))
                            .text_size(px(12.))
                            .role(Role::Button)
                            .aria_label("取消渲染样本")
                            .focusable()
                            .tab_stop(true)
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .active(move |d| d.bg(colors.color(ColorRole::Pressed)))
                            .focus_visible(move |d| d.shadow(vec![colors.focus_ring()]))
                            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.stop_propagation();
                                    cx.emit(ClosePanel);
                                }
                            }))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ClosePanel)))
                            .child("取消"),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(2.))
                    .bg(colors.color(ColorRole::Border))
                    .child(
                        div()
                            .h_full()
                            .w(relative(0.42108))
                            .bg(colors.color(ColorRole::Accent)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .mt(px(8.))
                    .mb(px(crate::layout::LAYOUT.panel.section_gap))
                    .text_size(px(12.))
                    .text_color(colors.color(ColorRole::Secondary))
                    .child(div().w(relative(0.5)).child("42,108 / 100,000 项"))
                    .child(div().flex_1().child("9.8 GB / 20.0 GB")),
            )
            .when(self.expanded, |d| {
                d.children((0..8).map(|i| {
                    let name = if i == 0 {
                        "归档_42109_项目研究报告_完整文件名称与补充资料.pdf".to_owned()
                    } else {
                        format!("归档_{}.pdf", 42109 + i)
                    };
                    let active = i < self.active;
                    let status = if active {
                        "正在复制"
                    } else if i == 3 {
                        "无权访问"
                    } else {
                        "等待"
                    };
                    let color = if i == 3 && !active {
                        ColorRole::Negative
                    } else {
                        ColorRole::Secondary
                    };
                    div()
                        .id(("task-file", i))
                        .w_full()
                        .h(px(28.))
                        .role(Role::ListBoxOption)
                        .aria_label(format!("{name}，{status}"))
                        .tooltip(tooltip(format!("{name}\n{status}"), colors))
                        .when(active, |d| {
                            d.child(flowing_name(
                                name.clone(),
                                phase,
                                animate,
                                self.cache.clone(),
                                colors,
                            ))
                        })
                        .when(!active, |d| {
                            d.flex()
                                .items_center()
                                .text_size(px(13.))
                                .text_color(colors.color(color))
                                .child(div().truncate().child(name))
                        })
                }))
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, _, _| marker.borrow_mut().painted(),
                )
                .absolute()
                .left_0()
                .top_0()
                .size(px(1.)),
            )
    }
}
