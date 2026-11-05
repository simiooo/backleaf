use super::*;
#[derive(Default)]
pub struct WatchUi {
    pub monitor: Option<crate::watcher::Monitor>,
    pub applied: u64,
    pub dirty: bool,
    pub(super) search_epoch: u64,
    pub(super) retry: Option<Task<()>>,
    pub restore_offset: Option<f64>,
    pub restore_anchor: Option<(std::ffi::OsString, [u8; 33])>,
}
impl Backleaf {
    pub fn watch_stale(&self) -> bool {
        self.watch.dirty
            || self
                .watch
                .monitor
                .as_ref()
                .is_some_and(|m| m.epoch() != self.watch.applied)
    }
    pub fn mark_directory_dirty(&mut self, cx: &mut Context<Self>) {
        self.watch.dirty = true;
        if self.watch.retry.is_none() {
            self.watch.retry = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(450))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.watch.retry = None;
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }
    pub fn sync_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.source.finished()
            && let Some(offset) = self.watch.restore_offset.take()
        {
            self.set_scroll_offset(app_scroll::Kind::Files, offset, window, cx);
        }
        if !self.watch_stale() || self.options.fixture.is_some() {
            return;
        }
        if !self.navigation.window_active
            || self.session_locked
            || self.files.editor.is_some()
            || self.files.menu.is_some()
            || self.selection_busy()
            || self.drag.gesture.is_some()
            || self
                .ole
                .as_ref()
                .is_some_and(|site| site.busy() || site.pointer().is_some())
            || self.navigation.drag.is_some()
            || self.files.hub.active()
            || !self.source.finished()
            || self.watch.retry.is_some()
        {
            if self.navigation.window_active && !self.session_locked && self.watch.retry.is_none() {
                self.mark_directory_dirty(cx);
            }
            return;
        }
        if self.search.open {
            let epoch = self.watch.monitor.as_ref().map_or(0, |m| m.epoch());
            if self.watch.search_epoch != epoch {
                self.refresh_search();
                self.watch.search_epoch = epoch;
            }
            self.watch.dirty = true;
            // Search has its own generation; reconcile the underlying directory when it closes.
            self.watch.applied = self.watch.monitor.as_ref().map_or(0, |m| m.epoch());
            return;
        }
        let offset = self
            .scroll_geometry(app_scroll::Kind::Files, window)
            .map_or(0., |g| g.offset);
        // Preserve a selection only when it already has a verified file identity. Never
        // transfer an old ordinal to an arbitrary file after enumeration order changes.
        let anchor = self
            .details
            .as_ref()
            .filter(|d| self.selection.count() == 1 && self.selection.focus == Some(d.id))
            .and_then(|d| Some((d.path.file_name()?.to_owned(), d.identity?)));
        self.navigate(self.path.clone(), false, cx);
        self.watch.restore_offset = Some(offset);
        self.watch.restore_anchor = anchor;
        if let Some((name, identity)) = self.watch.restore_anchor.take() {
            self.source.restore_identity(name, identity);
        }
    }
}
