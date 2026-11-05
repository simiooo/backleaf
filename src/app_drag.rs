use super::*;
use crate::operations::move_guard::Target;
pub struct Gesture {
    pub id: usize,
    pub start: Point<Pixels>,
    pub pointer: Point<Pixels>,
    pub active: bool,
    pub selection: Option<model::FrozenSelection>,
    pub target_id: Option<usize>,
    pub target: Option<Result<Target, String>>,
}
pub struct DragUi {
    pub worker: crate::drag::Worker,
    pub token: u64,
    pub gesture: Option<Gesture>,
}
impl DragUi {
    pub fn new(worker: crate::drag::Worker) -> Self {
        Self {
            worker,
            token: 0,
            gesture: None,
        }
    }
}
impl Backleaf {
    pub fn cancel_file_drag(&mut self) {
        if self.drag.gesture.take().is_some_and(|g| g.active) {
            self.suppress_click = true;
        }
        self.drag.token = self.drag.token.wrapping_add(1);
        self.drag.worker.cancel(self.drag.token);
    }
    pub fn begin_file_drag(&mut self, id: usize, event: &MouseDownEvent) {
        self.cancel_file_drag();
        if self.options.fixture.is_some() || self.files.editor.is_some() || self.watch_stale() {
            return;
        }
        self.drag.gesture = Some(Gesture {
            id,
            start: event.position,
            pointer: event.position,
            active: false,
            selection: None,
            target_id: None,
            target: None,
        });
    }
    pub fn move_file_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.drag.gesture.as_mut() else {
            return;
        };
        if !event.dragging() {
            self.cancel_file_drag();
            cx.notify();
            return;
        }
        g.pointer = event.position;
        if !g.active {
            let delta = event.position - g.start;
            if f32::from(delta.x).hypot(f32::from(delta.y)) < 6. {
                return;
            }
            let id = g.id;
            if !self.selection.contains(id) {
                self.selection.select(id, false, false);
            }
            g.selection = self.source.freeze_selection(self.selection.mask.clone());
            if g.selection.is_none() {
                self.cancel_file_drag();
                return;
            }
            g.active = true;
            self.suppress_click = true;
            self.pressed_id = None;
        }
        // Enter OLE as soon as the gesture is committed. Waiting for the pointer to
        // leave our bounds misses overlapping windows and can retarget a drop behind them.
        if self.ole.is_some() {
            let selection = self.drag.gesture.take().and_then(|g| g.selection);
            if let Some(selection) = selection
                && let Some(site) = &self.ole
                && let Err(error) = site.start(selection)
            {
                self.navigation_error(error.to_string(), cx);
            }
            self.suppress_click = true;
            cx.notify();
            return;
        }
        let target_id = self.hit_file(event.position, window).filter(|id| {
            !self.selection.contains(*id) && self.cache.get(id).is_some_and(|i| i.directory)
        });
        let g = self.drag.gesture.as_mut().unwrap();
        if target_id != g.target_id {
            self.drag.token = self.drag.token.wrapping_add(1);
            self.drag.worker.cancel(self.drag.token);
            g.target_id = target_id;
            g.target = None;
            if let Some(id) = target_id {
                let path = self.path.join(&self.cache[&id].os_name);
                self.drag.worker.request(crate::drag::Request {
                    token: self.drag.token,
                    generation: self.generation,
                    selection: g.selection.clone().unwrap(),
                    path,
                });
            }
        }
        cx.notify();
    }
    pub fn finish_file_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.drag.gesture.take() else {
            return;
        };
        self.drag.token = self.drag.token.wrapping_add(1);
        self.drag.worker.cancel(self.drag.token);
        if !g.active {
            return;
        }
        self.suppress_click = true;
        if self.watch_stale() {
            self.navigation_error("目录已改变，请重新拖放".into(), cx);
            return;
        }
        if self.pin_dragged_folder(&g, event.position, cx) {
            cx.notify();
            return;
        }
        if self.hit_file(event.position, window) != g.target_id || g.target_id.is_none() {
            return;
        }
        let target = match g.target {
            Some(Ok(target)) => target,
            Some(Err(error)) => {
                self.navigation_error(error, cx);
                return;
            }
            None => {
                self.navigation_error("正在检查目标，请重试".into(), cx);
                return;
            }
        };
        if let Some(selection) = g.selection {
            let source_volume = selection
                .snapshot
                .parent_identity
                .get()
                .and_then(|raw| operations::windows::identity::Identity::decode(raw))
                .map(|id| id.volume);
            let keys =
                u32::from(event.modifiers.control) * 8 + u32::from(event.modifiers.shift) * 4;
            let effect = crate::ole::effect(keys, 3, source_volume, Some(target.resolved.volume));
            if effect == 0 {
                self.navigation_error("暂不支持创建链接".into(), cx);
                return;
            }
            match self.files.hub.selection_guarded(
                selection,
                if effect == 2 {
                    operations::service::Operation::Move
                } else {
                    operations::service::Operation::Copy
                },
                target.path.clone(),
                self.native_owner_handle,
                Some(target),
            ) {
                Ok(id) => self.show_operation(id, cx),
                Err(error) => self.navigation_error(error.to_string(), cx),
            }
        }
        cx.notify();
    }
    pub fn drag_overlay(&self, window: &Window) -> AnyElement {
        let Some(g) = self.drag.gesture.as_ref().filter(|g| g.active) else {
            return div().into_any_element();
        };
        let label = match &g.target {
            Some(Ok(t)) => format!(
                "移动 {} 项 → {}",
                number(self.selection.count()),
                crate::navigation::display_name(&t.path)
            ),
            Some(Err(_)) => "无法移到此位置".into(),
            None if g.target_id.is_some() => "正在检查目标…".into(),
            None if f32::from(g.pointer.x) < self.sidebar_width() => "固定到侧栏".into(),
            None => format!("移动 {} 项", number(self.selection.count())),
        };
        div()
            .absolute()
            .left(px((f32::from(g.pointer.x) + 14.).min(
                (f32::from(window.viewport_size().width) - 280.).max(0.),
            )))
            .top(px((f32::from(g.pointer.y) + 18.).min(
                (f32::from(window.viewport_size().height) - 56.).max(0.),
            )))
            .max_w(px(264.))
            .px(px(10.))
            .py(px(14.))
            .rounded(px(12.))
            .bg(self.colors.color(ColorRole::Floating))
            .shadow(self.colors.control_shadows(false))
            .child(div().truncate().child(label))
            .into_any_element()
    }
}
