use super::*;
use crate::{
    menu::{Command, FileMenu, MenuEvent},
    quick_access::{Action, Destination, Entry},
};
pub struct QuickDrag {
    pub entry: Entry,
    pub start: Point<Pixels>,
    pub pointer: Point<Pixels>,
    pub active: bool,
    pub slot: usize,
}
impl Backleaf {
    pub fn quick_slot(&self, pointer: Point<Pixels>) -> usize {
        let y = f32::from(pointer.y)
            - layout::CHROME.bar_height
            - layout::SCROLL.sidebar_padding_y
            - f32::from(self.navigation.scroll.offset().y);
        ((y / (layout::SCROLL.sidebar_row_height + layout::SCROLL.sidebar_row_gap) + 0.5)
            .floor()
            .max(0.) as usize)
            .min(self.navigation.common.len())
    }
    pub fn quick_before(&self, slot: usize) -> Option<String> {
        self.navigation
            .common
            .get(slot)
            .and_then(|l| l.entry.as_ref())
            .map(|e| e.id.clone())
    }
    pub fn begin_quick_drag(&mut self, entry: Entry, event: &MouseDownEvent) {
        if self.files.editor.is_some() {
            return;
        }
        self.navigation.suppress_click = false;
        self.navigation.drag = Some(QuickDrag {
            entry,
            start: event.position,
            pointer: event.position,
            active: false,
            slot: self.quick_slot(event.position),
        });
    }
    pub fn move_quick_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            self.navigation.drag = None;
            cx.notify();
            return;
        }
        let slot = self.quick_slot(event.position);
        if let Some(g) = &mut self.navigation.drag {
            g.pointer = event.position;
            g.slot = slot;
            let delta = event.position - g.start;
            if f32::from(delta.x).hypot(f32::from(delta.y)) >= 6. {
                g.active = true;
                self.navigation.suppress_click = true;
            }
        }
        cx.notify();
    }
    pub fn finish_quick_drag(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(g) = self.navigation.drag.take() else {
            return;
        };
        if !g.active {
            return;
        }
        self.navigation.suppress_click = true;
        if f32::from(event.position.x) > self.sidebar_width()
            || f32::from(event.position.y) < layout::CHROME.bar_height
        {
            return;
        }
        self.reorder_quick(
            &g.entry.id,
            self.quick_before(self.quick_slot(event.position)),
            cx,
        );
    }
    fn reorder_quick(&mut self, id: &str, before: Option<String>, cx: &mut Context<Self>) {
        if before.as_deref() == Some(id) {
            return;
        }
        let mut ids = self
            .navigation
            .common
            .iter()
            .filter_map(|l| l.entry.as_ref().map(|e| e.id.clone()))
            .collect::<Vec<_>>();
        let Some(old) = ids.iter().position(|s| s == id) else {
            return;
        };
        let value = ids.remove(old);
        let at = before
            .and_then(|b| ids.iter().position(|s| *s == b))
            .unwrap_or(ids.len());
        ids.insert(at, value);
        if let Err(error) = self.navigation.store.send(Action::Reorder {
            ids,
            revision: self.navigation.config_revision,
        }) {
            self.navigation_error(error, cx);
        }
    }
    pub fn move_quick_by(&mut self, entry: &Entry, down: bool, cx: &mut Context<Self>) {
        let Some(index) = self
            .navigation
            .common
            .iter()
            .position(|l| l.entry.as_ref().is_some_and(|e| e.id == entry.id))
        else {
            return;
        };
        if !down && index == 0 || down && index + 1 >= self.navigation.common.len() {
            return;
        }
        let before = self.quick_before(if down { index + 2 } else { index - 1 });
        self.reorder_quick(&entry.id, before, cx);
    }
    pub fn quick_key(
        &mut self,
        entry: &Entry,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if m.alt && !m.control && (key == "up" || key == "down") {
            self.move_quick_by(entry, key == "down", cx);
            return true;
        }
        if !m.control && !m.alt && (key == "menu" || (key == "f10" && m.shift)) {
            self.show_quick_menu(
                entry.clone(),
                point(px(12.), px(layout::CHROME.bar_height + 40.)),
                true,
                window,
                cx,
            );
            return true;
        }
        false
    }
    pub fn show_quick_menu(
        &mut self,
        entry: Entry,
        position: Point<Pixels>,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.defer_quick_menu(entry.clone(), position, keyboard, cx) {
            return;
        }
        self.close_path_tree();
        self.files.menu = None;
        let mut commands = vec![Command::Open, Command::MoveQuickUp, Command::MoveQuickDown];
        if matches!(entry.destination, Destination::View(_)) {
            commands.push(Command::RenameQuickView);
        } else {
            commands.push(Command::CopyPath);
        }
        commands.push(Command::RemoveQuickAccess);
        let menu = cx.new(|cx| FileMenu::custom(self.colors, position, commands, keyboard, cx));
        self.navigation.menu_subscription = Some(cx.subscribe_in(
            &menu,
            window,
            move |this, _, event: &MenuEvent, window, cx| {
                this.navigation.menu = None;
                window.focus(&this.focus, cx);
                if let MenuEvent::Command(command) = event {
                    match command {
                        Command::Open => match &entry.destination {
                            Destination::Folder(p) => this.request_location(p.path(), cx),
                            Destination::View(v) => {
                                this.open_saved_view(entry.clone(), v.clone(), window, cx)
                            }
                        },
                        Command::MoveQuickUp => this.move_quick_by(&entry, false, cx),
                        Command::MoveQuickDown => this.move_quick_by(&entry, true, cx),
                        Command::RemoveQuickAccess => {
                            if let Err(error) = this.navigation.store.send(Action::Remove {
                                id: entry.id.clone(),
                                revision: entry.revision,
                            }) {
                                this.navigation_error(error, cx);
                            }
                        }
                        Command::RenameQuickView => {
                            if let Destination::View(v) = &entry.destination {
                                this.open_saved_view(entry.clone(), v.clone(), window, cx);
                                this.begin_save_view(false, window, cx);
                            }
                        }
                        Command::CopyPath => {
                            if let Destination::Folder(p) = &entry.destination
                                && let Err(error) =
                                    crate::locations::copy_path(p.path(), this.tx.clone())
                            {
                                this.navigation_error(error, cx);
                            }
                        }
                        _ => {}
                    }
                }
                cx.notify();
            },
        ));
        window.focus(&menu.focus_handle(cx), cx);
        self.navigation.menu = Some(menu);
        cx.notify();
    }
    pub fn pin_dragged_folder(
        &mut self,
        g: &app_drag::Gesture,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if f32::from(position.x) >= self.sidebar_width()
            || f32::from(position.y) < layout::CHROME.bar_height
        {
            return false;
        }
        if self.selection.count() != 1 {
            self.navigation_error("一次固定一个文件夹".into(), cx);
            return true;
        }
        if let Some(item) = self.cache.get(&g.id).filter(|item| item.directory) {
            let action = Action::Pin {
                path: self.path.join(&item.os_name),
                title: item.name.clone(),
                before: self.quick_before(self.quick_slot(position)),
                revision: self.navigation.config_revision,
            };
            if let Err(error) = self.navigation.store.send(action) {
                self.navigation_error(error, cx);
            }
        } else {
            self.navigation_error("只能固定文件夹".into(), cx);
        }
        true
    }
    pub fn quick_drag_velocity(&self, window: &Window) -> f64 {
        if self.session_locked || !window.is_window_active() || !window.is_visible() {
            return 0.;
        }
        let pointer = self
            .navigation
            .drag
            .as_ref()
            .filter(|g| g.active)
            .map(|g| g.pointer)
            .or_else(|| {
                self.drag
                    .gesture
                    .as_ref()
                    .filter(|g| g.active && f32::from(g.pointer.x) < self.sidebar_width())
                    .map(|g| g.pointer)
            });
        let Some(pointer) = pointer else {
            return 0.;
        };
        let Some(geo) = self.scroll_geometry(app_scroll::Kind::Sidebar, window) else {
            return 0.;
        };
        let y = f32::from(pointer.y);
        let top = layout::CHROME.bar_height + layout::SCROLL.sidebar_padding_y;
        let bottom = f32::from(window.viewport_size().height) - layout::SCROLL.sidebar_padding_y;
        if y < top + 32. && geo.offset > 0. {
            -f64::from(120. + (top + 32. - y).clamp(0., 64.) * 10.)
        } else if y > bottom - 32. && geo.offset < geo.maximum {
            f64::from(120. + (y - bottom + 32.).clamp(0., 64.) * 10.)
        } else {
            0.
        }
    }
    pub fn step_quick_drag(&mut self, window: &Window, cx: &mut Context<Self>) {
        let speed = self.quick_drag_velocity(window);
        if speed == 0. {
            self.navigation.drag_tick = Instant::now();
            return;
        }
        let dt = self.navigation.drag_tick.elapsed().as_secs_f64().min(0.05);
        self.navigation.drag_tick = Instant::now();
        if let Some(g) = self.scroll_geometry(app_scroll::Kind::Sidebar, window) {
            self.set_scroll_offset(app_scroll::Kind::Sidebar, g.offset + speed * dt, window, cx);
        }
        if let Some(pointer) = self.navigation.drag.as_ref().map(|g| g.pointer) {
            let slot = self.quick_slot(pointer);
            if let Some(g) = &mut self.navigation.drag {
                g.slot = slot;
            }
        }
    }
    pub fn quick_drag_overlay(&self, window: &Window) -> AnyElement {
        let gesture = self.navigation.drag.as_ref().filter(|g| g.active);
        let file = self
            .drag
            .gesture
            .as_ref()
            .filter(|g| g.active && f32::from(g.pointer.x) < self.sidebar_width());
        let Some((pointer, slot)) = gesture
            .map(|g| (g.pointer, g.slot))
            .or_else(|| file.map(|g| (g.pointer, self.quick_slot(g.pointer))))
        else {
            return div().into_any_element();
        };
        let top = layout::CHROME.bar_height
            + layout::SCROLL.sidebar_padding_y
            + slot as f32 * (layout::SCROLL.sidebar_row_height + layout::SCROLL.sidebar_row_gap)
            + f32::from(self.navigation.scroll.offset().y);
        let line = div()
            .absolute()
            .left(px(12.))
            .top(px(top.clamp(
                layout::CHROME.bar_height + 8.,
                f32::from(window.viewport_size().height) - 8.,
            )))
            .w(px((self.sidebar_width() - 24.).max(0.)))
            .h(px(2.))
            .rounded(px(1.))
            .bg(self.colors.color(ColorRole::Focus));
        let preview = gesture.map(|g| {
            div()
                .absolute()
                .left(px((f32::from(pointer.x) + 14.).max(12.)))
                .top(px(f32::from(pointer.y) + 18.))
                .max_w(px(264.))
                .px(px(10.))
                .py(px(14.))
                .rounded(px(12.))
                .bg(self.colors.color(ColorRole::Floating))
                .shadow(self.colors.control_shadows(false))
                .child(div().truncate().child(g.entry.title.clone()))
        });
        div()
            .absolute()
            .inset_0()
            .child(line)
            .children(preview)
            .into_any_element()
    }
}
