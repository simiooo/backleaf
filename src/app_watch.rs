use super::*;
#[derive(Default)]
pub struct WatchUi {
    pub monitor: Option<crate::watcher::Monitor>,
    pub applied: u64,
    pub dirty: bool,
    pub(super) search_epoch: u64,
    pub(super) retry: Option<Task<()>>,
    pub generation: u64,
    pub pending: Option<Refresh>,
    pub locate: Option<(std::ffi::OsString, bool)>,
}
impl Backleaf {
    pub fn watch_stale(&self) -> bool {
        self.watch.pending.is_some()
            || self.watch.dirty
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
        if (!self.watch_stale() && self.watch.pending.is_none()) || self.options.fixture.is_some() {
            return;
        }
        if !self.navigation.window_active
            || self.session_locked
            || self.files.editor.is_some()
            || self.detail_pending
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
        if self.watch.pending.is_none() {
            self.next_generation += 1;
            self.watch.pending = Some(Refresh {
                source: Source::directory(self.path.clone(), self.next_generation, self.tx.clone()),
                generation: self.next_generation,
                epoch: self.watch.monitor.as_ref().map_or(0, |m| m.epoch()),
                capture: None,
                ready: None,
            });
            self.watch.dirty = false;
        }
        if !self.watch.pending.as_ref().unwrap().source.finished() {
            return;
        }
        let offset = self
            .scroll_geometry(app_scroll::Kind::Files, window)
            .map_or(0., |g| g.offset);
        let valid = self
            .watch
            .pending
            .as_ref()
            .unwrap()
            .capture
            .as_ref()
            .is_some_and(|c| {
                c.selection.mask.same_snapshot(&self.selection.mask)
                    && c.selection.focus == self.selection.focus
                    && c.selection.anchor == self.selection.anchor
                    && c.columns == self.columns
                    && (c.offset - offset).abs() < 0.5
            });
        if !valid {
            let Some(old) = self.source.freeze_selection(self.selection.mask.clone()) else {
                return;
            };
            let columns = self.columns.max(1);
            let top = (offset / f64::from(layout::row_step())).floor() as usize * columns;
            let request = model::refresh::Request {
                old,
                focus: self.selection.focus,
                anchor: self.selection.anchor,
                top,
                columns,
                page_len: self.requested.len(),
                cut: self
                    .files
                    .hub
                    .own_clipboard()
                    .filter(|c| c.cut)
                    .map(|c| c.ledger.as_ref().clone()),
            };
            let p = self.watch.pending.as_mut().unwrap();
            // One request at a time. If the user changed selection/scroll while it ran,
            // discard that result and prepare the current viewport on the same worker.
            if p.capture.is_none() || p.ready.is_some() {
                p.capture = Some(Capture {
                    selection: self.selection.clone(),
                    offset,
                    columns,
                });
                p.ready = None;
                p.source.refresh(request);
            }
            return;
        }
        if self.watch.pending.as_ref().unwrap().ready.is_none() {
            return;
        }
        let mut pending = self.watch.pending.take().unwrap();
        let ready = pending.ready.take().unwrap();
        let fraction = offset % f64::from(layout::row_step());
        self.source = pending.source;
        self.generation = pending.generation;
        self.watch.applied = pending.epoch;
        self.serial += 1;
        self.requested = ready.range;
        self.cache = ready
            .items
            .into_iter()
            .map(|item| (item.id, Arc::new(item)))
            .collect();
        self.labels.borrow_mut().clear();
        self.selection = ready.selection;
        self.files.removed.clear();
        self.hovered_id = None;
        self.pressed_id = None;
        self.detail_token += 1;
        self.detail_pending = false;
        if let Some(id) = self.selection.focus {
            if self.details.is_some() {
                self.detail_pending = true;
                self.source.detail(self.detail_token, id);
            }
        } else {
            self.details = None;
            self.detail_name = None;
            self.detail_path = None;
        }
        self.set_scroll_offset(
            app_scroll::Kind::Files,
            (ready.top / self.columns.max(1)) as f64 * f64::from(layout::row_step()) + fraction,
            window,
            cx,
        );
        if let Some((name, rename)) = self.watch.locate.take() {
            self.files.locate_token += 1;
            self.files.locate_rename = rename;
            self.source.locate(self.files.locate_token, name);
        }
        if self.watch_stale() {
            self.mark_directory_dirty(cx);
        }
        cx.notify();
    }
    pub fn refresh_count(
        &mut self,
        count: usize,
        done: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.refresh_failed(error, cx);
            return;
        }
        if let Some(Refresh {
            source:
                Source::Directory {
                    count: current,
                    done: finished,
                    ..
                },
            ..
        }) = &mut self.watch.pending
        {
            *current = (*current).max(count);
            *finished = done;
        }
        cx.notify();
    }
    pub fn refresh_failed(&mut self, error: String, cx: &mut Context<Self>) {
        let epoch = self
            .watch
            .pending
            .take()
            .map_or(self.watch.applied, |p| p.epoch);
        self.watch.applied = epoch;
        self.watch.dirty = false;
        self.watch.locate = None;
        self.navigation_error(format!("刷新失败：{error}"), cx);
    }
}
pub struct Refresh {
    pub source: Source,
    pub generation: u64,
    epoch: u64,
    capture: Option<Capture>,
    pub ready: Option<model::refresh::Ready>,
}
struct Capture {
    selection: Selection,
    offset: f64,
    columns: usize,
}
