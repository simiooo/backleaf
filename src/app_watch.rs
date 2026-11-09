use super::*;
#[derive(Default)]
pub struct WatchUi {
    pub monitor: Option<crate::watcher::Monitor>,
    pub applied: u64,
    pub dirty: bool,
    /// A user/operation-initiated refresh that must not wait for the watcher
    /// debounce or for the window to be activated. Cleared when committed or
    /// failed.
    pub immediate: bool,
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
        if self.watch.retry.is_none() && !self.watch.immediate {
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
    /// A completed in-app file operation does not need watcher debounce: refresh on
    /// the next frame and drop the debounce timer so removed tiles do not linger as
    /// empty placeholders. The refresh may start while the window is inactive so a
    /// dropped/pasted file appears without waiting for activation.
    pub fn mark_directory_dirty_now(&mut self, cx: &mut Context<Self>) {
        self.watch.dirty = true;
        self.watch.retry = None;
        self.watch.immediate = true;
        cx.notify();
    }
    pub fn sync_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if (!self.watch_stale() && self.watch.pending.is_none()) || self.options.fixture.is_some() {
            return;
        }
        if (!self.watch.immediate && !self.navigation.window_active)
            || self.session_locked
            || self.files.editor.is_some()
            || self.detail_pending
            || self.typeahead.pending
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
            // An immediate refresh stays pending without arming the watcher
            // debounce; otherwise the 450ms timer would start during the running
            // operation and delay its post-completion refresh.
            if !self.watch.immediate
                && self.navigation.window_active
                && !self.session_locked
                && self.watch.retry.is_none()
            {
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
            self.watch.immediate = false;
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
        self.watch.immediate = false;
        self.hovered_id = None;
        self.pressed_id = None;
        self.detail_token += 1;
        self.detail_pending = false;
        // A previously suspended watcher (denied/offline location) comes back
        // only after an explicit retry actually succeeded.
        if self.watch.monitor.is_none() && self.options.fixture.is_none() {
            let monitor =
                watcher::Monitor::new(self.path.clone(), self.generation, self.tx.clone());
            self.watch.applied = monitor.epoch();
            self.watch.monitor = Some(monitor);
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
        denied: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            if denied {
                self.refresh_denied(error, cx);
            } else {
                self.refresh_failed(error, cx);
            }
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
        self.suspend_watch();
        self.navigation_error(format!("刷新失败：{error}"), cx);
    }
    /// A failed directory load cannot be watched: `FindFirstChangeNotificationW`
    /// keeps failing and the periodic identity probe marks the dir dirty, which
    /// would refresh a denied/unavailable directory forever in the background.
    /// Suspend automatic refresh until the user explicitly retries.
    pub fn suspend_watch(&mut self) {
        let epoch = self
            .watch
            .pending
            .take()
            .map_or(self.watch.applied, |p| p.epoch);
        self.watch.applied = epoch;
        self.watch.monitor = None;
        self.watch.retry = None;
        self.watch.dirty = false;
        self.watch.immediate = false;
        self.watch.locate = None;
    }
    /// A refresh can be refused after the folder was already reachable (ACL or
    /// share changes). Keep the old rows and offer the administrator handoff
    /// instead of only flashing a toast.
    pub fn refresh_denied(&mut self, error: String, cx: &mut Context<Self>) {
        self.suspend_watch();
        if self.elevation.elevated {
            // The elevated token is already the strongest offer; surface why it
            // still failed instead of silently leaving the old rows.
            self.navigation_error(format!("刷新失败：{error}"), cx);
        } else {
            self.elevation_prompt(self.path.clone(), cx);
            if self.elevation.prompt.is_none() {
                // The prompt is suppressed (another handoff is pending, a close
                // guard is up, or this is a fixture window); keep the failure
                // visible instead of dropping it.
                self.navigation_error(format!("刷新失败：{error}"), cx);
            }
        }
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
