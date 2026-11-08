use super::*;
use crate::scrollbar::Geometry;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Files,
    Sidebar,
    Content,
}
#[derive(Clone, Copy)]
pub struct Drag {
    pub kind: Kind,
    pointer: f32,
    geometry: Geometry,
}
impl Backleaf {
    /// Content results use the same logical card metrics as the overlay. The
    /// estimate tracks padding, toolbar, scope row and gaps; the bar is rendered
    /// next to the list so a small rounding difference is harmless.
    fn content_scroll_metrics(&self, window: &Window) -> (f64, f64, f64, f32, f32) {
        let viewport = window.viewport_size();
        let width =
            f32::from(viewport.width) - self.sidebar_width() - 2. * layout::SURFACE.margin_x;
        let inner_width = (width - 32.).max(1.);
        let columns = (((inner_width + 12.) / (260. + 12.)).floor() as usize).max(1);
        let rows = self.search.content_results.len().div_ceil(columns);
        let list_height = (f32::from(viewport.height)
            - layout::CHROME.bar_height
            - 2. * layout::SURFACE.margin_y
            - 90.)
            .max(0.);
        let list_top = layout::SURFACE.margin_y + 78.;
        let content = rows as f64 * 160.;
        let offset = -f64::from(f32::from(
            self.search.content_scroll.0.borrow().base_handle.offset().y,
        ));
        (
            f64::from(list_height),
            content,
            offset,
            list_top,
            list_height,
        )
    }
    pub fn scroll_geometry(&self, kind: Kind, window: &Window) -> Option<Geometry> {
        let s = &layout::SCROLL;
        let viewport = (f32::from(window.viewport_size().height)
            - layout::CHROME.bar_height
            - 2. * s.inset_y
            - if kind == Kind::Files {
                2. * layout::SURFACE.margin_y
            } else {
                0.
            })
        .max(0.);
        let (content, offset) = match kind {
            Kind::Files => (
                self.source.count().div_ceil(self.columns.max(1)) as f64
                    * f64::from(layout::row_step()),
                self.origin_row as f64 * f64::from(layout::row_step())
                    - f64::from(f32::from(self.scroll.0.borrow().base_handle.offset().y)),
            ),
            Kind::Sidebar => (
                (self.navigation.common.len() as f32 * s.sidebar_row_height
                    + self.navigation.volumes.len() as f32 * s.volume_row_height
                    + (self.navigation.common.len() + self.navigation.volumes.len())
                        .saturating_sub(1) as f32
                        * s.sidebar_row_gap)
                    .max(0.) as f64,
                -f64::from(f32::from(self.navigation.scroll.offset().y)),
            ),
            Kind::Content => {
                if !self.search.content_mode {
                    return None;
                }
                let (viewport, content, offset, _, _) = self.content_scroll_metrics(window);
                return Geometry::new(viewport, content, offset, s.minimum_thumb_height);
            }
        };
        Geometry::new(f64::from(viewport), content, offset, s.minimum_thumb_height)
    }
    pub(super) fn set_scroll_offset(
        &mut self,
        kind: Kind,
        value: f64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.scroll_geometry(kind, window) else {
            return;
        };
        let value = value.clamp(0., g.maximum);
        match kind {
            Kind::Files => {
                let rows = self.source.count().div_ceil(self.columns.max(1));
                let (origin, local) =
                    crate::scrollbar::rebase(value, rows, f64::from(layout::row_step()));
                self.origin_row = origin;
                let mut state = self.scroll.0.borrow_mut();
                state.deferred_scroll_to_item = None;
                state.base_handle.set_offset(point(px(0.), px(-local)));
                drop(state);
                self.hovered_id = None;
                self.grid.update(cx, |_, cx| cx.notify());
            }
            Kind::Sidebar => self
                .navigation
                .scroll
                .set_offset(point(px(0.), px(-(value as f32)))),
            Kind::Content => {
                let state = self.search.content_scroll.0.borrow_mut();
                state
                    .base_handle
                    .set_offset(point(px(0.), px(-(value as f32))));
                drop(state);
            }
        }
        cx.notify();
    }
    pub fn drag_scrollbar(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.scrollbar_drag else {
            return;
        };
        if !event.dragging()
            || self
                .scroll_geometry(drag.kind, window)
                .is_none_or(|g| (g.viewport - drag.geometry.viewport).abs() > 0.5)
        {
            self.scrollbar_drag = None;
            cx.notify();
            return;
        }
        let value = drag
            .geometry
            .dragged_offset(f32::from(event.position.y) - drag.pointer);
        self.set_scroll_offset(drag.kind, value, window, cx);
    }
    fn begin_scrollbar(
        &mut self,
        kind: Kind,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.scroll_geometry(kind, window) else {
            return;
        };
        self.cancel_marquee();
        self.pressed_id = None;
        let pointer = f32::from(event.position.y);
        let track_top = match kind {
            Kind::Content => layout::CHROME.bar_height + layout::SURFACE.margin_y + 78.,
            _ => {
                layout::CHROME.bar_height
                    + layout::SCROLL.inset_y
                    + if kind == Kind::Files {
                        layout::SURFACE.margin_y
                    } else {
                        0.
                    }
            }
        };
        let local = pointer - track_top;
        if local >= g.top && local <= g.top + g.thumb {
            self.scrollbar_drag = Some(Drag {
                kind,
                pointer,
                geometry: g,
            });
        } else {
            let page = if local < g.top {
                -g.viewport
            } else {
                g.viewport
            };
            self.set_scroll_offset(kind, g.offset + page, window, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub fn scrollbar(
        &self,
        kind: Kind,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let g = self.scroll_geometry(kind, window)?;
        let dragging = self.scrollbar_drag.is_some_and(|d| d.kind == kind);
        if kind == Kind::Sidebar && !self.navigation.hovered && !dragging {
            return None;
        }
        let active = dragging || self.scrollbar_hover == Some(kind);
        let colors = self.colors;
        let s = &layout::SCROLL;
        let thumb_width = if active {
            s.active_thumb_width
        } else {
            s.thumb_width
        };
        let opacity = if active {
            s.active_thumb_opacity
        } else {
            s.thumb_opacity
        };
        let owner = cx.entity().downgrade();
        let content_metrics = (kind == Kind::Content).then(|| self.content_scroll_metrics(window));
        let mut bar = div()
            .id(match kind {
                Kind::Files => "file-scrollbar",
                Kind::Sidebar => "sidebar-scrollbar",
                Kind::Content => "content-search-scrollbar",
            })
            .absolute()
            .right(px(if kind == Kind::Content {
                layout::SURFACE.margin_x + s.rail_inset
            } else {
                s.rail_inset
            }))
            .w(px(s.rail_width))
            .occlude()
            .rounded(px(6.))
            .role(Role::ScrollBar)
            .aria_label(match kind {
                Kind::Files => "文件滚动条",
                Kind::Sidebar => "侧栏滚动条",
                Kind::Content => "内容搜索结果滚动条",
            })
            .aria_orientation(accesskit::Orientation::Vertical)
            .aria_min_numeric_value(0.)
            .aria_max_numeric_value(100.)
            .aria_numeric_value(g.offset / g.maximum * 100.)
            .focusable()
            .tab_stop(kind != Kind::Sidebar)
            .focus_visible(move |d| d.bg(colors.color(ColorRole::Hover)))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.scrollbar_hover = Some(kind);
                } else if this.scrollbar_hover == Some(kind) {
                    this.scrollbar_hover = None;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.begin_scrollbar(kind, event, window, cx)
                }),
            )
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, window, cx| {
                if let Some(g) = this.scroll_geometry(kind, window) {
                    let delta =
                        f64::from(f32::from(event.delta.pixel_delta(window.line_height()).y));
                    this.set_scroll_offset(kind, g.offset - delta, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let Some(g) = this.scroll_geometry(kind, window) else {
                    return;
                };
                let value = match event.keystroke.key.as_str() {
                    "up" => g.offset - 32.,
                    "down" => g.offset + 32.,
                    "pageup" => g.offset - g.viewport,
                    "pagedown" => g.offset + g.viewport,
                    "home" => 0.,
                    "end" => g.maximum,
                    _ => return,
                };
                this.set_scroll_offset(kind, value, window, cx);
                cx.stop_propagation();
            }))
            .on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                if let Some(accesskit::ActionData::NumericValue(value)) = data
                    && value.is_finite()
                {
                    let _ = owner.update(cx, |this, cx| {
                        if let Some(g) = this.scroll_geometry(kind, window) {
                            this.set_scroll_offset(
                                kind,
                                g.maximum * value.clamp(0., 100.) / 100.,
                                window,
                                cx,
                            );
                        }
                    });
                }
            })
            .child(
                div()
                    .absolute()
                    .top(px(g.top))
                    .left(px((s.rail_width - thumb_width) / 2.))
                    .w(px(thumb_width))
                    .h(px(g.thumb))
                    .rounded(px(thumb_width / 2.))
                    .bg(colors.color(ColorRole::Secondary).opacity(opacity)),
            );
        bar = match content_metrics {
            Some((_, _, _, top, height)) => bar.top(px(top)).h(px(height)),
            None => bar.top(px(s.inset_y)).bottom(px(s.inset_y)),
        };
        Some(bar.into_any_element())
    }
}
