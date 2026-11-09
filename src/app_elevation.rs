//! "This location needs an administrator" prompt and the elevated window banner.
//!
//! The normal window stays untouched. Confirming the prompt writes a one-shot
//! [`crate::elevation::Request`] and starts a second Backleaf process through
//! UAC; that process reopens the same location (the retry), restores the window
//! intent and keeps the elevated token until its window closes.
use super::*;
use crate::{elevation, file_view::WinPath, selection::SelectionMask};
use std::path::PathBuf;
pub struct ElevationUi {
    pub elevated: bool,
    pub banner: bool,
    pub prompt: Option<ElevationPrompt>,
    pub pending: bool,
    pub restore: Option<elevation::RestoreState>,
}
pub struct ElevationPrompt {
    pub path: PathBuf,
}
impl ElevationUi {
    pub fn new(restore: Option<elevation::RestoreState>) -> Self {
        let elevated = elevation::is_elevated();
        Self {
            elevated,
            banner: elevated,
            prompt: None,
            pending: false,
            restore,
        }
    }
}
impl Backleaf {
    /// Raise the administrator prompt for `path`. Elevated or fixture windows
    /// never prompt: a second prompt cannot acquire more access.
    pub fn elevation_prompt(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.elevation.elevated
            || self.elevation.pending
            || self.options.fixture.is_some()
            || self.files.close_guard
        {
            return;
        }
        if self
            .elevation
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.path == path)
        {
            return;
        }
        self.elevation.prompt = Some(ElevationPrompt { path });
        self.error = Some(crate::i18n::tr_owned("此位置需要管理员权限"));
        cx.notify();
    }
    pub fn elevation_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.elevation.prompt = None;
        // The list stays empty: keep the reason visible in the status line.
        self.error = Some(crate::i18n::tr_owned("此位置需要管理员权限"));
        window.focus(&self.focus, cx);
        cx.notify();
    }
    /// Capture the window intent and start the UAC handoff off the UI thread;
    /// the UAC prompt blocks until the user answers.
    pub fn elevation_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self
            .elevation
            .prompt
            .as_ref()
            .map(|prompt| prompt.path.clone())
        else {
            return;
        };
        self.elevation_start(target, window, cx);
    }
    /// Open a folder in a new elevated window. Shared by the permission prompt
    /// and the explicit right-click "以管理员身份打开" command.
    pub fn elevation_start(
        &mut self,
        target: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.elevation.elevated {
            self.navigation_error(crate::i18n::tr_owned("此窗口已是管理员权限"), cx);
            return;
        }
        if self.elevation.pending || self.options.fixture.is_some() {
            return;
        }
        let from_menu = self.elevation.prompt.is_none();
        if from_menu {
            self.error = Some(crate::i18n::tr_owned("正在请求管理员权限…"));
        }
        let same_directory = self.path == target;
        let mut state = self.capture_window_state(window, cx);
        state.path = WinPath::new(&target);
        // A failed or replaced listing cannot be retried by reopening a search.
        state.search = None;
        let handoff = same_directory
            .then(|| {
                self.source
                    .selection_handoff(&self.selection.mask, self.source.count())
            })
            .flatten();
        let scroll_row = if same_directory {
            self.elevation_scroll_row()
        } else {
            0
        };
        self.elevation.pending = true;
        let state_dir = self.options.state_dir.clone();
        let tx = self.tx.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("backleaf-elevation".into())
            .spawn(move || {
                let selection = handoff
                    .as_ref()
                    .map(elevation::capture_selection)
                    .unwrap_or_default();
                let request = elevation::Request::new(state, selection, scroll_row);
                let result = elevation::write_request(&state_dir, &request).and_then(|path| {
                    match elevation::launch_elevated(&path, &state_dir) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            // A cancelled UAC prompt leaves nothing to consume.
                            let _ = std::fs::remove_file(&path);
                            Err(error)
                        }
                    }
                });
                let _ = tx.send_blocking(Event::ElevationLaunched(result));
            })
        {
            self.elevation.pending = false;
            self.navigation_error(error.to_string(), cx);
        }
        cx.notify();
    }
    /// Right-click command: open the focused folder / current folder elevated.
    pub fn elevation_open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.elevation.elevated {
            self.navigation_error(crate::i18n::tr_owned("此窗口已是管理员权限"), cx);
            return;
        }
        let target = if self.selection.count() == 0 {
            self.path.clone()
        } else if self.selection.count() == 1 {
            let Some(id) = self
                .selection
                .focus
                .filter(|id| self.selection.contains(*id))
            else {
                return;
            };
            match self.cache.get(&id) {
                Some(item) if item.directory => self.path.join(item.name.clone()),
                _ => {
                    self.navigation_error(crate::i18n::tr_owned("请选择一个文件夹"), cx);
                    return;
                }
            }
        } else {
            return;
        };
        self.elevation_start(target, window, cx);
    }
    fn elevation_scroll_row(&self) -> u64 {
        let offset = -f32::from(self.scroll.0.borrow().base_handle.offset().y) as f64;
        let local = (offset.max(0.) / f64::from(layout::row_step())).floor() as u64;
        self.origin_row as u64 + local
    }
    pub fn accept_elevation_launch(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.elevation.pending = false;
        match result {
            Ok(()) => {
                self.elevation.prompt = None;
                self.error = Some(crate::i18n::tr_owned("已打开管理员窗口"));
            }
            Err(error) => {
                if self.elevation.prompt.is_none() {
                    // Menu-initiated attempt has no modal to explain the failure.
                    self.error = None;
                }
                // Keep the prompt so the user can retry after cancelling UAC.
                self.navigation_error(error, cx);
            }
        }
        cx.notify();
    }
    pub fn elevation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.elevation.prompt.is_none() {
            return false;
        }
        if event.is_held {
            cx.stop_propagation();
            return true;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.elevation_dismiss(window, cx),
            "enter" => self.elevation_confirm(window, cx),
            _ => {}
        }
        cx.stop_propagation();
        true
    }
    /// Restore selection/scroll carried by the handoff once the elevated window
    /// finished enumerating the retried location.
    pub fn elevation_begin_restore(&mut self) {
        let Some(restore) = self.elevation.restore.take() else {
            return;
        };
        if restore.selection.is_empty() && restore.scroll_row == 0 {
            return;
        }
        let Some(database) = self.source.snapshot_database() else {
            return;
        };
        let generation = self.generation;
        let tx = self.tx.clone();
        let thread = std::thread::Builder::new()
            .name("backleaf-elevation-restore".into())
            .spawn(move || {
                let ids = elevation::resolve_selection(&database, &restore.selection);
                let _ = tx.send_blocking(Event::ElevationSelection {
                    generation,
                    ids,
                    scroll_row: restore.scroll_row,
                });
            });
        let _ = thread;
    }
    pub fn accept_elevation_selection(
        &mut self,
        generation: u64,
        ids: Vec<usize>,
        scroll_row: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        if !ids.is_empty() {
            let mut values = roaring::RoaringTreemap::new();
            for id in &ids {
                values.insert(*id as u64);
            }
            self.selection.mask = SelectionMask::from_bitmap(values);
            self.selection.focus = ids.first().copied();
            self.selection.anchor = ids.first().copied().unwrap_or(0);
            self.keyboard_focus = false;
        }
        if let Some(first) = ids.first() {
            let row = first / self.columns.max(1);
            self.scroll_global(row, ScrollStrategy::Center, false);
        } else if scroll_row > 0 {
            self.scroll_global(scroll_row as usize, ScrollStrategy::Center, false);
        }
        cx.notify();
    }
    #[cfg(test)]
    pub fn dismiss_elevation_banner(&mut self, cx: &mut Context<Self>) {
        self.elevation.banner = false;
        cx.notify();
    }
    pub fn elevation_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(prompt) = &self.elevation.prompt {
            return self.elevation_modal(prompt, window, cx);
        }
        if self.elevation.banner {
            return self.elevation_banner();
        }
        div().into_any_element()
    }
    fn elevation_modal(
        &self,
        prompt: &ElevationPrompt,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors;
        let width = 480_f32.min(f32::from(window.viewport_size().width) - 48.);
        let path = navigation::display_path(&prompt.path);
        let confirm = if self.elevation.pending {
            crate::i18n::tr("正在请求管理员权限…")
        } else {
            crate::i18n::tr("以管理员身份重新打开")
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.color(ColorRole::Canvas).opacity(0.72))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("elevation-permission-dialog")
                    .w(px(width))
                    .px(px(20.))
                    .py(px(20.))
                    .rounded(px(12.))
                    .bg(colors.color(ColorRole::Floating))
                    .border_1()
                    .border_color(colors.color(ColorRole::Border))
                    .shadow(colors.control_shadows(false))
                    .text_color(colors.color(ColorRole::Primary))
                    .role(Role::Alert)
                    .aria_label(crate::i18n::tr("需要管理员权限"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::i18n::tr("需要管理员权限")),
                    )
                    .child(
                        div()
                            .id("elevation-target")
                            .mt(px(10.))
                            .text_size(px(12.))
                            .text_color(colors.color(ColorRole::Secondary))
                            .truncate()
                            .tooltip(tooltip(navigation::display_path(&prompt.path), colors))
                            .child(path),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(12.))
                            .text_color(colors.color(ColorRole::Secondary))
                            .child(crate::i18n::tr(
                                "此位置需要管理员权限才能访问。Windows 会请求你的同意，并在新窗口中以管理员身份打开同一位置。",
                            )),
                    )
                    .child(
                        div()
                            .mt(px(20.))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id("elevation-cancel")
                                    .h(px(32.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.))
                                    .text_size(px(12.))
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::tr("取消"))
                                    .focusable()
                                    .tab_stop(true)
                                    .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                                    .active(move |d| d.bg(colors.color(ColorRole::Pressed)))
                                    .focus_visible(move |d| {
                                        d.bg(colors.color(ColorRole::InputActive))
                                            .shadow(colors.control_shadows(true))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.elevation_dismiss(window, cx)
                                    }))
                                    .child(crate::i18n::tr("取消")),
                            )
                            .child(
                                div()
                                    .id("elevation-confirm")
                                    .h(px(32.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.))
                                    .text_size(px(12.))
                                    .role(Role::Button)
                                    .aria_label(confirm)
                                    .focusable()
                                    .tab_stop(true)
                                    .when(!self.elevation.pending, |d| {
                                        d.bg(colors.color(ColorRole::Accent))
                                            .text_color(colors.color(ColorRole::OnAccent))
                                            .hover(move |d| {
                                                d.bg(colors.color(ColorRole::Focus))
                                            })
                                    })
                                    .when(self.elevation.pending, |d| {
                                        d.bg(colors.color(ColorRole::Disabled))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.elevation_confirm(window, cx)
                                    }))
                                    .child(confirm),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn elevation_banner(&self) -> AnyElement {
        let colors = self.colors;
        // Compact status marker in the title bar: a shield with a tooltip
        // explains the elevated token; no dismiss button or alert surface.
        div()
            .absolute()
            .right(px(150.))
            .top(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .w(px(28.))
            .h(px(28.))
            .id("elevation-shield")
            .role(Role::Status)
            .aria_label(crate::i18n::tr("管理员权限已开启"))
            .tooltip(tooltip(
                crate::i18n::tr(
                    "管理员权限已开启：此窗口以管理员身份运行；关闭窗口后管理员权限结束。",
                )
                .to_string(),
                colors,
            ))
            .child(div().text_size(px(15.)).child("🛡"))
            .into_any_element()
    }
}
