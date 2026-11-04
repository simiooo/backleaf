use crate::theme::{Palette, Role as ColorRole};
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Read-only, wrapped text with mouse/keyboard selection and an unchanged clipboard value.
pub struct Selectable {
    pub colors: &'static Palette,
    text: SharedString,
    focus: FocusHandle,
    selected: Range<usize>,
    anchor: usize,
    dragging: bool,
}
impl Selectable {
    pub fn new(text: String, colors: &'static Palette, cx: &mut Context<Self>) -> Self {
        Self {
            colors,
            text: text.into(),
            focus: cx.focus_handle(),
            selected: 0..0,
            anchor: 0,
            dragging: false,
        }
    }
    fn boundary(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i <= index)
            .last()
            .unwrap_or(0)
    }
}
impl Render for Selectable {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.selected.clone();
        let styled = StyledText::new(self.text.clone()).with_default_highlights(
            &window.text_style(),
            (!selection.is_empty()).then_some((
                selection,
                HighlightStyle {
                    background_color: Some(self.colors.color(ColorRole::Selected)),
                    color: Some(self.colors.color(ColorRole::OnSelected)),
                    ..Default::default()
                },
            )),
        );
        let down_layout = styled.layout().clone();
        let move_layout = down_layout.clone();
        div()
            .id("selectable")
            .track_focus(&self.focus)
            .role(Role::Label)
            .aria_value(self.text.clone())
            .w_full()
            .min_w_0()
            .cursor(CursorStyle::IBeam)
            .text_size(px(13.))
            .line_height(px(21.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                    let i = down_layout
                        .index_for_position(event.position)
                        .unwrap_or_else(|i| i)
                        .min(this.text.len());
                    let i = if i == this.text.len() {
                        i
                    } else {
                        this.boundary(i)
                    };
                    this.anchor = i;
                    this.selected = i..i;
                    this.dragging = true;
                    if event.click_count >= 2 {
                        this.selected = 0..this.text.len();
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.dragging {
                    let i = move_layout
                        .index_for_position(event.position)
                        .unwrap_or_else(|i| i)
                        .min(this.text.len());
                    let i = if i == this.text.len() {
                        i
                    } else {
                        this.boundary(i)
                    };
                    this.selected = this.anchor.min(i)..this.anchor.max(i);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.control {
                    match event.keystroke.key.as_str() {
                        "a" => {
                            this.selected = 0..this.text.len();
                            cx.stop_propagation();
                            cx.notify();
                        }
                        "c" => {
                            if !this.selected.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    this.text[this.selected.clone()].into(),
                                ));
                            }
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                }
            }))
            .child(styled)
    }
}
