use super::*;
use crate::window_state::{SearchState, State};
pub struct RestoreUi {
    pub store: Option<crate::window_state::Store>,
    pub revision: u64,
    pub last: Option<State>,
    pub pending_search: Option<SearchState>,
    pub closing: Option<u64>,
    pub ready_close: bool,
    pub closed: bool,
    pub close_requested: bool,
    pub restored: bool,
    pub failed: bool,
    pub initial_error: Option<String>,
}
impl Backleaf {
    pub fn capture_window_state(&self, window: &Window, cx: &App) -> State {
        let bounds = window.window_bounds();
        let rect = bounds.get_bounds();
        let search = if self.search.open {
            let mut definition = self.search.definition.clone();
            if let Some(input) = &self.search.input
                && !input.read(cx).is_composing()
            {
                definition.query = input.read(cx).value();
            }
            Some(SearchState {
                definition,
                saved: self.search.saved.clone(),
            })
        } else {
            self.restore.pending_search.clone()
        };
        State {
            version: 1,
            path: crate::file_view::WinPath::new(&self.path),
            search,
            bounds: [
                f32::from(rect.origin.x),
                f32::from(rect.origin.y),
                f32::from(rect.size.width),
                f32::from(rect.size.height),
            ],
            maximized: matches!(bounds, WindowBounds::Maximized(_)),
        }
    }
    pub fn sync_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(error) = self.restore.initial_error.take() {
            self.navigation_error(error, cx);
        }
        if self.restore.ready_close {
            self.restore.closed = true;
            window.remove_window();
            return;
        }
        if let Some(saved) = self.restore.pending_search.take() {
            self.open_search(window, cx);
            self.search.root = saved.definition.root.path();
            self.search.definition = saved.definition.clone();
            self.search.saved = saved.saved;
            if let Some(input) = &self.search.input {
                input.update(cx, |i, cx| {
                    i.restore_unconfirmed_composition(saved.definition.query.clone(), cx)
                });
            }
            self.start_search(saved.definition.query, cx);
        }
        if self.restore.close_requested
            && !self.view_inputs_pending(cx)
            && !self.archive_input_pending(cx)
            && !self
                .search
                .input
                .as_ref()
                .is_some_and(|i| i.read(cx).is_pasting() || i.read(cx).is_composing())
        {
            self.restore.close_requested = false;
            self.request_close(window, cx);
        }
        if self.restore.closing.is_none()
            && !self.restore.closed
            && let Some(store) = &self.restore.store
        {
            let state = self.capture_window_state(window, cx);
            if self.restore.last.as_ref() != Some(&state) {
                self.restore.revision += 1;
                store.save(self.restore.revision, state.clone(), false, None);
                self.restore.last = Some(state);
            }
        }
    }
    pub fn close_window_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restore.store.is_none() {
            window.remove_window();
            return;
        }
        if self.restore.closing.is_some() {
            return;
        }
        let state = self.capture_window_state(window, cx);
        self.restore.revision += 1;
        self.restore.closing = Some(self.restore.revision);
        self.restore.last = Some(state.clone());
        self.restore
            .store
            .as_ref()
            .unwrap()
            .save(self.restore.revision, state, true, None);
        cx.notify();
    }
    pub fn save_state_result(
        &mut self,
        revision: u64,
        closed: bool,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                if closed && self.restore.closing == Some(revision) {
                    self.restore.ready_close = true;
                }
            }
            Err(_) => {
                self.restore.closing = None;
                self.restore.close_requested = false;
                self.navigation_error(crate::i18n::tr_owned("无法保存窗口状态，请重试"), cx);
            }
        }
        cx.notify();
    }
    pub fn restore_failure(&self, _cx: &mut Context<Self>) -> AnyElement {
        if !self.restore.failed {
            return div().into_any_element();
        }
        // The unavailable-location message belongs to the empty content area:
        // centered as a block, left-aligned internally, with no card surface
        // and no interactive controls.
        let error = self.error.clone();
        div()
            .absolute()
            .left(px(self.sidebar_width()))
            .right(px(0.))
            .top(px(layout::CHROME.bar_height))
            .bottom(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.))
                    .max_w(px(480.))
                    .px(px(24.))
                    .child(crate::i18n::tr("此位置暂时不可用"))
                    .child(div().truncate().child(navigation::display_path(&self.path)))
                    .when_some(error, |d, error| d.child(div().child(error)))
                    .when(self.elevation.elevated, |d| {
                        d.child(crate::i18n::tr(
                            "管理员权限仍被拒绝；该位置可能需要系统级权限或获取所有权",
                        ))
                    }),
            )
            .into_any_element()
    }
}
