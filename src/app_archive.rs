use super::*;
use crate::{
    input::{InputEvent, TextInput},
    operations::archive::{Kind, validate_target},
};
pub struct Draft {
    input: Entity<TextInput>,
    _subscription: Subscription,
    selection: model::FrozenSelection,
    kind: Kind,
    parent: PathBuf,
    token: u64,
    pending: bool,
    focus: bool,
    error: Option<String>,
}
#[derive(Default)]
pub struct ArchiveUi {
    pub draft: Option<Draft>,
    serial: u64,
}
pub struct Prepared {
    pub selection: model::FrozenSelection,
    pub kind: Kind,
    pub parent: PathBuf,
    pub name: String,
    pub parent_identity: [u8; 33],
}
impl Backleaf {
    pub fn archive_input_pending(&self, cx: &App) -> bool {
        self.archive
            .draft
            .as_ref()
            .is_some_and(|d| d.pending || d.input.read(cx).is_pasting())
    }
    pub fn flush_archive_name(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(d) = &self.archive.draft else {
            return true;
        };
        if d.input.read(cx).is_composing() {
            let input = d.input.clone();
            self.archive.draft.as_mut().unwrap().focus = true;
            self.restore.close_requested = false;
            self.navigation_error("请先完成输入法候选".into(), cx);
            drop(input);
            return false;
        }
        self.restore.close_requested = true;
        if !d.pending && !d.input.read(cx).is_pasting() {
            let value = d.input.read(cx).value();
            self.submit_archive(value, cx);
        }
        false
    }
    pub fn begin_archive(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        crate::popup_menu::trace(&format!(
            "begin archive stale={} count={} active={}",
            self.watch_stale(),
            self.selection.count(),
            window.is_window_active()
        ));
        self.reset_typeahead();
        if self.defer_archive(kind, cx) {
            return;
        }
        if self.watch_stale() || self.selection.count() == 0 {
            return;
        }
        let Some(selection) = self.source.freeze_selection(self.selection.mask.clone()) else {
            return;
        };
        let parent = self.source.parent().unwrap_or_else(|| self.path.clone());
        let selected = self.selection.focus.and_then(|id| self.cache.get(&id));
        let base = if self.selection.count() == 1 {
            selected
                .map(|i| {
                    if i.directory {
                        i.name.clone()
                    } else {
                        std::path::Path::new(&i.name)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }
                })
                .unwrap_or_else(|| "归档".into())
        } else {
            if kind == Kind::Extract {
                "解压文件"
            } else {
                "归档"
            }
            .into()
        };
        let value = if kind == Kind::CreateZip {
            format!("{base}.zip")
        } else {
            base
        };
        let input = cx.new(|cx| {
            TextInput::new(
                value.clone(),
                0..value.len(),
                if kind == Kind::CreateZip {
                    "压缩包名称"
                } else {
                    "解压文件夹名称"
                },
                self.colors,
                255,
                cx,
            )
        });
        let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            match event {
                InputEvent::Submit(value) | InputEvent::Blur(value) => {
                    if input.read(cx).is_composing() {
                        if let Some(d) = &mut this.archive.draft {
                            d.focus = true;
                            d.error = Some("请先完成输入法候选".into());
                        }
                    } else {
                        this.submit_archive(value.clone(), cx);
                    }
                }
                InputEvent::Cancel => {
                    this.archive.draft = None;
                    this.files.focus_grid = true;
                }
                InputEvent::Error(error) => {
                    if let Some(d) = &mut this.archive.draft {
                        d.focus = true;
                        d.error = Some(error.clone());
                    }
                }
                _ => {}
            }
            cx.notify();
        });
        self.archive.serial += 1;
        self.archive.draft = Some(Draft {
            input: input.clone(),
            _subscription: subscription,
            selection,
            kind,
            parent,
            token: self.archive.serial,
            pending: false,
            focus: true,
            error: None,
        });
        window.focus(&input.focus_handle(cx), cx);
        cx.notify();
    }
    fn submit_archive(&mut self, mut name: String, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.archive.draft else {
            return;
        };
        if draft.pending {
            return;
        }
        if draft.kind == Kind::CreateZip && !name.to_lowercase().ends_with(".zip") {
            name.push_str(".zip");
        }
        draft.pending = true;
        draft.error = None;
        draft.input.update(cx, |i, cx| {
            i.enabled = false;
            cx.notify();
        });
        let (token, selection, kind, parent, tx) = (
            draft.token,
            draft.selection.clone(),
            draft.kind,
            draft.parent.clone(),
            self.tx.clone(),
        );
        std::thread::spawn(move || {
            let result = (|| -> Result<Prepared, String> {
                validate_target(&parent, &name)?;
                let parent_identity = operations::windows::identity::capture(&parent)
                    .map_err(|e| e.to_string())?
                    .identity
                    .encode();
                if kind == Kind::Extract {
                    let db = rusqlite::Connection::open_with_flags(
                        &selection.snapshot.database,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .map_err(|e| e.to_string())?;
                    let mut q = db
                        .prepare("SELECT dir FROM files WHERE id=?1")
                        .map_err(|e| e.to_string())?;
                    for id in selection.mask.ids_from(0, selection.count) {
                        let directory = q
                            .query_row([id as i64], |r| r.get::<_, bool>(0))
                            .map_err(|e| e.to_string())?;
                        if directory {
                            return Err("请选择压缩包文件".into());
                        }
                    }
                }
                Ok(Prepared {
                    selection,
                    kind,
                    parent,
                    name,
                    parent_identity,
                })
            })();
            let _ = tx.send_blocking(Event::ArchivePrepared { token, result });
        });
    }
    pub fn accept_archive(
        &mut self,
        token: u64,
        result: Result<Prepared, String>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .archive
            .draft
            .as_ref()
            .is_some_and(|d| d.token == token)
        {
            return;
        }
        let result = result.and_then(|p| {
            self.files
                .hub
                .archive(p.selection, p.kind, p.parent, p.name, p.parent_identity)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(id) => {
                self.archive.draft = None;
                self.show_operation(id);
                self.files.focus_grid = true;
            }
            Err(error) => {
                self.restore.close_requested = false;
                if let Some(d) = &mut self.archive.draft {
                    d.pending = false;
                    d.focus = true;
                    d.error = Some(error.clone());
                    d.input.update(cx, |i, cx| {
                        i.enabled = true;
                        cx.notify();
                    });
                }
                self.navigation_error(error, cx);
            }
        }
        cx.notify();
    }
    pub fn sync_archive_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active()
            && let Some(d) = &mut self.archive.draft
            && d.focus
        {
            d.focus = false;
            window.focus(&d.input.focus_handle(cx), cx);
        }
    }
    pub fn archive_popup(&self, _cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = &self.archive.draft else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .right(px(144.))
            .top(px(layout::CHROME.bar_height + 8.))
            .w(px(360.))
            .max_w_full()
            .px(px(12.))
            .py(px(16.))
            .rounded(px(12.))
            .bg(self.colors.color(ColorRole::Floating))
            .shadow(self.colors.control_shadows(false))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("archive-parent")
                    .truncate()
                    .text_size(px(12.))
                    .text_color(self.colors.color(ColorRole::Secondary))
                    .tooltip(tooltip(navigation::display_path(&d.parent), self.colors))
                    .child(navigation::display_path(&d.parent)),
            )
            .child(if d.kind == Kind::CreateZip {
                "压缩包名称"
            } else {
                "解压文件夹名称"
            })
            .child(d.input.clone())
            .children(d.error.as_ref().map(|error| {
                div()
                    .id("archive-error")
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .text_size(px(12.))
                    .text_color(self.colors.color(ColorRole::Negative))
                    .child(error.clone())
            }))
            .into_any_element()
    }
}
