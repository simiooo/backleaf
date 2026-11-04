//! Native single-line input, adapted from the pinned GPUI Apache-2.0 input example.
//! IME ranges remain UTF-16; editing, selection and horizontal scrolling use shaped text.
use crate::theme::{Palette, Role as ColorRole};
use gpui::{prelude::*, *};
use std::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};
use unicode_segmentation::*;
actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Quit,
    ]
);

pub struct TextInput {
    pub colors: &'static Palette,
    pub enabled: bool,
    pub embedded: bool,
    max_utf16: usize,
    label: SharedString,
    scroll: Pixels,
    epoch: u64,
    paste_pending: bool,
    notify_input: bool,
    blur_subscription: Option<Subscription>,
    blur_pending: bool,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    pub fn value(&self) -> String {
        self.content.to_string()
    }
    pub fn is_composing(&self) -> bool {
        self.marked_range.as_ref().is_some_and(|r| !r.is_empty())
    }
    pub fn is_pasting(&self) -> bool {
        self.paste_pending
    }
    pub fn without_composition(&self) -> Option<String> {
        self.marked_range
            .as_ref()
            .filter(|r| !r.is_empty())
            .map(|r| self.content[..r.start].to_owned() + &self.content[r.end..])
    }
    pub fn restore_unconfirmed_composition(&mut self, value: String, cx: &mut Context<Self>) {
        self.content = value.into();
        self.marked_range = None;
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.blur_pending = false;
        self.notify_input = true;
        self.epoch += 1;
        cx.notify();
    }
    pub fn set_placeholder(&mut self, value: &'static str) {
        self.placeholder = value.into();
    }
    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(self.selected_range.start, window, cx)
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), window, cx);
        } else {
            self.move_to(self.selected_range.end, window, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
        self.select_to(self.content.len(), window, cx)
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                window.play_system_bell();
                return;
            }
            self.select_to(prev, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        window.request_virtual_keyboard();

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), window, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled || self.paste_pending {
            return;
        }
        if crate::locations::path_copy_pending() {
            cx.emit(InputEvent::Error("正在复制路径，请稍后粘贴".into()));
            return;
        }
        if TEXT_READ_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            cx.emit(InputEvent::Error("正在读取剪贴板".into()));
            return;
        }
        self.paste_pending = true;
        let epoch = self.epoch;
        let range = self
            .marked_range
            .clone()
            .unwrap_or(self.selected_range.clone());
        let maximum = self.max_utf16;
        let (tx, rx) = async_channel::bounded(1);
        let worker = std::thread::Builder::new()
            .name("backleaf-text-clipboard".into())
            .spawn(move || {
                let _guard = TextReadGuard;
                let value = crate::operations::clipboard::Clipboard::new()
                    .and_then(|clipboard| clipboard.read_text_limited(maximum))
                    .map_err(|e| e.to_string());
                let _ = tx.send_blocking(value);
            });
        if let Err(error) = worker {
            TEXT_READ_ACTIVE.store(false, Ordering::Release);
            self.paste_pending = false;
            cx.emit(InputEvent::Error(error.to_string()));
            return;
        }
        cx.spawn(async move |this, cx| {
            if let Ok(value) = rx.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.paste_pending = false;
                    if this.epoch != epoch || !this.enabled {
                        this.finish_pending_blur(cx);
                        return;
                    }
                    match value {
                        Ok(Some(text)) => {
                            let text = text.replace(['\r', '\n'], " ");
                            let value = this.content[..range.start].to_owned()
                                + &text
                                + &this.content[range.end..];
                            if value.encode_utf16().count() > this.max_utf16 {
                                this.blur_pending = false;
                                cx.emit(InputEvent::Error("粘贴的文字过长".into()));
                                return;
                            }
                            this.content = value.into();
                            this.selected_range =
                                range.start + text.len()..range.start + text.len();
                            this.marked_range = None;
                            this.selection_reversed = false;
                            this.epoch += 1;
                            this.notify_input = true;
                            cx.emit(InputEvent::Changed);
                            this.finish_pending_blur(cx);
                            cx.notify();
                        }
                        Ok(None) => this.finish_pending_blur(cx),
                        Err(error) => {
                            this.blur_pending = false;
                            cx.emit(InputEvent::Error(error));
                        }
                    }
                });
            }
        })
        .detach();
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll)
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let a = self.offset_from_utf16(range_utf16.start);
        let b = self.offset_from_utf16(range_utf16.end);
        a.min(b)..a.max(b)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = self.range_from_utf16(&range_utf16);
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.take().is_some() {
            cx.emit(InputEvent::Changed);
        }
        self.finish_pending_blur(cx);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let next = self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        if next.encode_utf16().count() > self.max_utf16 {
            cx.emit(InputEvent::Error("文字过长".into()));
            return;
        }
        self.content = next.into();
        self.epoch += 1;
        cx.emit(InputEvent::Changed);
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.marked_range.take();
        self.finish_pending_blur(cx);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let next = self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        if next.encode_utf16().count() > self.max_utf16 {
            cx.emit(InputEvent::Error("文字过长".into()));
            return;
        }
        self.content = next.into();
        self.epoch += 1;
        cx.emit(InputEvent::Changed);
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        let insertion_start_utf16 = self.offset_to_utf16(range.start);
        let inserted_length_utf16 = new_text.encode_utf16().count();
        self.selected_range = new_selected_range_utf16
            .map(|selected| {
                self.range_from_utf16(
                    &(insertion_start_utf16 + selected.start.min(inserted_length_utf16)
                        ..insertion_start_utf16 + selected.end.min(inserted_length_utf16)),
                )
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;

        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        if last_layout.text != self.content {
            return None;
        }
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - self.scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - self.scroll,
                bounds.bottom(),
            ),
        ))
    }

    fn selection_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range_utf16, bounds, window, cx)
            .into_iter()
            .collect()
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        if last_layout.text != self.content {
            return None;
        }
        let utf8_index = last_layout.closest_index_for_x(line_point.x + self.scroll);
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (
                input.placeholder.clone(),
                input.colors.color(ColorRole::Secondary),
            )
        } else {
            (content, style.color)
        };

        let focused = window.is_window_active() && input.focus_handle.is_focused(window);
        let mut breaks = vec![
            0,
            display_text.len(),
            selected_range.start,
            selected_range.end,
        ];
        if let Some(marked) = &input.marked_range {
            breaks.extend([marked.start, marked.end]);
        }
        breaks.sort_unstable();
        breaks.dedup();
        let runs = breaks
            .windows(2)
            .filter(|w| w[0] < w[1] && w[1] <= display_text.len())
            .map(|w| TextRun {
                len: w[1] - w[0],
                font: style.font(),
                color: if selected_range.contains(&w[0]) {
                    input.colors.color(if focused {
                        ColorRole::OnAccent
                    } else {
                        ColorRole::OnInactiveSelected
                    })
                } else {
                    text_color
                },
                background_color: None,
                underline: input
                    .marked_range
                    .as_ref()
                    .filter(|r| r.contains(&w[0]))
                    .map(|_| UnderlineStyle {
                        color: Some(input.colors.color(ColorRole::Primary)),
                        thickness: px(1.),
                        wavy: false,
                    }),
                strikethrough: None,
            })
            .collect::<Vec<_>>();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let maximum = (line.width - bounds.size.width + px(2.)).max(px(0.));
        let mut scroll = input.scroll.clamp(px(0.), maximum);
        if cursor_pos - scroll > bounds.size.width - px(2.) {
            scroll = (cursor_pos - bounds.size.width + px(2.)).max(px(0.));
        }
        if cursor_pos < scroll {
            scroll = cursor_pos;
        }
        scroll = scroll.clamp(px(0.), maximum);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos - scroll, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    input.colors.color(ColorRole::Focus),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start) - scroll,
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end) - scroll,
                            bounds.bottom(),
                        ),
                    ),
                    input.colors.color(if focused {
                        ColorRole::Accent
                    } else {
                        ColorRole::InactiveSelected
                    }),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            scroll,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let line = prepaint.line.take().unwrap();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            let _ = line.paint(
                point(bounds.left() - prepaint.scroll, bounds.top()),
                window.line_height(),
                TextAlign::Left,
                None,
                window,
                cx,
            );
            if window.is_window_active()
                && focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll = prepaint.scroll;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.notify_input {
            self.notify_input = false;
            window.notify_text_input_changed(&self.focus_handle);
        }
        let colors = self.colors;
        if self.blur_subscription.is_none() {
            self.blur_subscription = Some(cx.on_blur(&self.focus_handle, window, |this, _, cx| {
                if this.enabled {
                    this.blur_pending = true;
                    if this.marked_range.is_some() && !this.paste_pending {
                        cx.emit(InputEvent::Blur(this.content.to_string()));
                    } else {
                        this.finish_pending_blur(cx);
                    }
                }
                cx.notify();
            }));
        }
        let active = window.is_window_active() && self.focus_handle.is_focused(window);
        let accessible_input = cx.entity().downgrade();
        div()
            .id("native-input")
            .key_context("BackleafInput")
            .track_focus(&self.focus_handle)
            .tab_stop(true)
            .role(Role::TextInput)
            .aria_label(self.label.clone())
            .aria_value(self.content.clone())
            .on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                if let Some(accesskit::ActionData::Value(value)) = data {
                    let _ = accessible_input.update(cx, |this, cx| {
                        let range = 0..this.content.encode_utf16().count();
                        this.replace_text_in_range(Some(range), value, window, cx);
                    });
                }
            })
            .w_full()
            .min_w_0()
            .h(px(32.))
            .px(px(if self.embedded { 0. } else { 4. }))
            .flex()
            .items_center()
            .overflow_hidden()
            .when(!self.embedded, |d| {
                d.bg(colors.color(if active {
                    ColorRole::InputActive
                } else {
                    ColorRole::InputRest
                }))
                .border_1()
                .border_color(colors.color(ColorRole::Border))
                .when(active, |d| d.shadow(colors.input_shadows()))
            })
            .rounded(px(8.))
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(colors.color(ColorRole::Primary))
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.platform || event.keystroke.modifiers.alt {
                    return;
                }
                if event.keystroke.key == "enter" && this.marked_range.is_none() {
                    cx.stop_propagation();
                    if this.enabled && !event.is_held {
                        if this.paste_pending {
                            this.blur_pending = true;
                        } else {
                            cx.emit(InputEvent::Submit(this.content.to_string()));
                        }
                    }
                } else if event.keystroke.key == "escape" && this.marked_range.is_none() {
                    cx.stop_propagation();
                    cx.emit(InputEvent::Cancel);
                }
            }))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.on_mouse_down(event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    Blur(String),
    Submit(String),
    Cancel,
    Changed,
    Error(String),
}
impl EventEmitter<InputEvent> for TextInput {}
static TEXT_READ_ACTIVE: AtomicBool = AtomicBool::new(false);
struct TextReadGuard;
impl Drop for TextReadGuard {
    fn drop(&mut self) {
        TEXT_READ_ACTIVE.store(false, Ordering::Release);
    }
}
impl TextInput {
    pub fn new(
        value: String,
        selected: Range<usize>,
        label: &'static str,
        colors: &'static Palette,
        max_utf16: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            colors,
            enabled: true,
            embedded: false,
            max_utf16,
            label: label.into(),
            scroll: px(0.),
            epoch: 0,
            paste_pending: false,
            notify_input: false,
            blur_subscription: None,
            blur_pending: false,
            focus_handle: cx.focus_handle().tab_stop(true),
            content: value.into(),
            placeholder: "".into(),
            selected_range: selected,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }
    pub fn select_full(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }
    pub fn wait_for_composition_blur(&mut self) {
        self.blur_pending = true;
    }
    fn finish_pending_blur(&mut self, cx: &mut Context<Self>) {
        if self.enabled && self.blur_pending && self.marked_range.is_none() && !self.paste_pending {
            self.blur_pending = false;
            cx.emit(InputEvent::Blur(self.content.to_string()));
        }
    }
}
// Run on the UI executor outside an App/Window update: IMM delivers completion
// synchronously through GPUI's current input adapter. Never complete another window.
pub fn complete_native_composition(owner: usize) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::Input::{
            Ime::{
                CPS_COMPLETE, ImmGetContext, ImmNotifyIME, ImmReleaseContext, NI_COMPOSITIONSTR,
            },
            KeyboardAndMouse::GetFocus,
        },
    };
    let window = HWND(owner as *mut std::ffi::c_void);
    unsafe {
        if GetFocus() != window {
            return false;
        }
        let context = ImmGetContext(window);
        if context.is_invalid() {
            return false;
        }
        let completed = ImmNotifyIME(context, NI_COMPOSITIONSTR, CPS_COMPLETE, 0).as_bool();
        let _ = ImmReleaseContext(window, context);
        completed
    }
}
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("BackleafInput")),
        KeyBinding::new("delete", Delete, Some("BackleafInput")),
        KeyBinding::new("left", Left, Some("BackleafInput")),
        KeyBinding::new("right", Right, Some("BackleafInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("BackleafInput")),
        KeyBinding::new("shift-right", SelectRight, Some("BackleafInput")),
        KeyBinding::new("home", Home, Some("BackleafInput")),
        KeyBinding::new("end", End, Some("BackleafInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("BackleafInput")),
        KeyBinding::new("ctrl-c", Copy, Some("BackleafInput")),
        KeyBinding::new("ctrl-x", Cut, Some("BackleafInput")),
        KeyBinding::new("ctrl-v", Paste, Some("BackleafInput")),
    ]);
}
