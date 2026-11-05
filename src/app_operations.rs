//! Window-facing file commands. Every filesystem/clipboard scan stays on the bounded service.
use super::*;
use crate::{
    input::{InputEvent, TextInput},
    menu::{Command, FileMenu, MenuEvent},
    operations::{
        ledger::State,
        service::{Hub, Operation, Phase, View},
        windows::identity::Identity,
    },
};
use roaring::RoaringTreemap;
use std::{collections::HashSet, ffi::OsString};

pub struct FileOperations {
    ime_completion_inflight: bool,
    after_edit: Option<AfterEdit>,
    menu_after_locate: Option<(u64, Point<Pixels>, PathBuf)>,
    ready_menu: Option<(Point<Pixels>, usize)>,
    refresh_pending: bool,
    pub hub: Hub,
    pub menu: Option<crate::menu::OwnedMenu>,
    menu_subscription: Option<Subscription>,
    menu_generation: u64,
    pub editor: Option<Editor>,
    editor_subscription: Option<Subscription>,
    pub open: bool,
    selected: Option<u64>,
    expanded: bool,
    failed_only: bool,
    page_after: u64,
    page_history: Vec<u64>,
    page_requested: Option<(u64, u64, bool, u64, bool)>,
    pub removed: RoaringTreemap,
    seen: HashSet<u64>,
    clipboard_sequence: u32,
    pub detail_intent: DetailIntent,
    pub locate_token: u64,
    pub locate_rename: bool,
    pub focus_grid: bool,
    new_folder: Option<u64>,
    rename_location: Option<(PathBuf, OsString)>,
    pub close_guard: bool,
    pub exit_after_cancel: bool,
}
enum AfterEdit {
    Navigate(PathBuf, bool),
    Menu {
        position: Point<Pixels>,
        name: Option<OsString>,
        parent: PathBuf,
    },
    Select {
        name: Option<OsString>,
        parent: PathBuf,
    },
    Close,
    Search,
    EditPathAt(PathBuf),
    Location(PathBuf),
    BreadcrumbMenu {
        path: PathBuf,
        position: Point<Pixels>,
        keyboard: bool,
    },
    PathTree(bool),
    Archive(crate::operations::archive::Kind),
    QuickMenu(crate::quick_access::Entry, Point<Pixels>, bool),
    SavedView(crate::quick_access::Entry, crate::file_view::FileView),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailIntent {
    Show,
    Open,
    Rename,
    PasteFolder,
}
pub enum EditKind {
    Path,
    Rename(Detail),
}
pub struct Editor {
    pub input: Entity<TextInput>,
    pub kind: EditKind,
    pub error: Option<String>,
    pending: Option<u64>,
    resolving: bool,
    token: u64,
    pub focus: bool,
    blurred: bool,
}
impl FileOperations {
    pub fn new(hub: Hub) -> Self {
        Self {
            ime_completion_inflight: false,
            after_edit: None,
            menu_after_locate: None,
            ready_menu: None,
            refresh_pending: false,
            hub,
            menu: None,
            menu_subscription: None,
            menu_generation: 0,
            editor: None,
            editor_subscription: None,
            open: false,
            selected: None,
            expanded: true,
            failed_only: false,
            page_after: 0,
            page_history: vec![],
            page_requested: None,
            removed: RoaringTreemap::new(),
            seen: HashSet::new(),
            clipboard_sequence: 0,
            detail_intent: DetailIntent::Show,
            locate_token: 0,
            locate_rename: false,
            focus_grid: false,
            new_folder: None,
            rename_location: None,
            close_guard: false,
            exit_after_cancel: false,
        }
    }
    pub fn animating(&self) -> bool {
        self.open
            && self
                .selected
                .and_then(|id| self.hub.view(id))
                .is_some_and(|v| v.phase == Phase::Running && !v.active.is_empty())
    }
}
impl Backleaf {
    pub fn native_owner(window: &Window) -> usize {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(h) = handle.as_raw()
        {
            h.hwnd.get() as usize
        } else {
            0
        }
    }
    pub fn refresh_directory(
        &mut self,
        name: Option<OsString>,
        rename: bool,
        cx: &mut Context<Self>,
    ) {
        self.mark_directory_dirty(cx);
        if let Some(name) = name {
            if rename {
                self.files.rename_location = Some((self.path.clone(), name.clone()));
            }
            self.watch.locate = Some((name, rename));
        }
    }
    pub fn refresh_cut(&mut self, cx: &mut Context<Self>) {
        self.files.hub.observe_clipboard();
        let range = self.requested.clone();
        self.requested = 0..0;
        self.ensure_page(range);
        cx.notify();
    }
    pub fn operations_changed(&mut self, cx: &mut Context<Self>) {
        let views = self.files.hub.views();
        let mut refresh = false;
        let mut locate = None;
        let mut rename = false;
        for v in &views {
            if v.source_mask.is_some()
                && self
                    .source
                    .snapshot_path()
                    .is_some_and(|p| v.source_snapshot.as_ref().is_some_and(|s| s.as_path() == p))
            {
                self.files.removed |= v.removed.as_ref();
            }
            if !v.phase.terminal() {
                continue;
            }
            if self
                .files
                .editor
                .as_ref()
                .is_some_and(|e| e.pending == Some(v.id))
            {
                if v.phase == Phase::Complete {
                    let blurred = self.files.editor.as_ref().is_some_and(|e| e.blurred);
                    locate = v
                        .created
                        .as_ref()
                        .and_then(|p| p.file_name().map(OsString::from));
                    if let Some(AfterEdit::Menu{name:Some(name),..})=&mut self.files.after_edit
                        && self.files.editor.as_ref().is_some_and(|e|matches!(&e.kind,EditKind::Rename(d)if d.path.file_name()==Some(name.as_os_str())))
                        && let Some(created)=&locate {*name=created.clone();}
                    self.files.editor = None;
                    self.files.editor_subscription = None;
                    self.files.focus_grid = !blurred;
                    refresh = true;
                } else if let Some(editor) = &mut self.files.editor {
                    self.files.after_edit = None;
                    editor.pending = None;
                    editor.error = Some(
                        v.error
                            .as_deref()
                            .unwrap_or("无法重命名，请检查此位置")
                            .to_owned(),
                    );
                    editor.focus = true;
                    editor.input.update(cx, |input, cx| {
                        input.enabled = true;
                        cx.notify();
                    });
                }
            }
            if self.files.new_folder == Some(v.id) {
                self.files.new_folder = None;
                if v.phase == Phase::Complete
                    // Compare the original destination, not Shell's returned
                    // display spelling (which may expand a DOS 8.3 parent).
                    && same_location(&v.target, &self.path)
                {
                    locate = v
                        .created
                        .as_ref()
                        .and_then(|p| p.file_name().map(OsString::from));
                    rename = true;
                    refresh = true;
                }
            }
            if self.files.seen.insert(v.id)
                && matches!(
                    v.operation,
                    Operation::Copy
                        | Operation::Move
                        | Operation::Recycle
                        | Operation::Rename
                        | Operation::NewFolder
                )
            {
                // Refresh once per completed task; never rebuild a huge directory for every copied item.
                refresh |= same_location(&v.target, &self.path)
                    || v.source_mask.is_some()
                        && self.source.snapshot_path().is_some_and(|p| {
                            v.source_snapshot.as_ref().is_some_and(|s| s.as_path() == p)
                        });
            }
            if self.files.selected == Some(v.id)
                && matches!(v.phase, Phase::Complete | Phase::Canceled)
            {
                self.files.open = false;
                self.files.selected = None;
            }
            if v.operation == Operation::NewFolder
                && v.phase != Phase::Complete
                && let Some(error) = &v.error
            {
                self.error = Some(error.to_string());
            }
        }
        self.selection.mask.exclude(&self.files.removed);
        if self.visible_operations().is_empty() {
            self.files.open = false;
            self.files.selected = None;
        }
        if self.files.seen.len() > 128 {
            self.files
                .seen
                .retain(|id| views.iter().any(|v| v.id == *id));
        }
        if refresh && self.files.editor.is_some() {
            self.files.refresh_pending = true;
        } else if refresh {
            self.refresh_directory(locate, rename, cx);
        }
        let sequence = operations::clipboard::Clipboard::sequence();
        if sequence != self.files.clipboard_sequence {
            self.files.clipboard_sequence = sequence;
            self.refresh_cut(cx);
        }
        self.request_operation_page();
        cx.notify();
    }
    pub fn open_menu(
        &mut self,
        position: Point<Pixels>,
        id: Option<usize>,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::popup_menu::trace("file open_menu");
        self.close_path_tree();
        self.navigation.menu = None;
        if self.files.editor.is_some() {
            let name = if let Some(id) = id {
                let Some(item) = self.cache.get(&id) else {
                    self.error = Some("文件尚未加载，请重试".into());
                    cx.notify();
                    return;
                };
                Some(item.os_name.clone())
            } else {
                None
            };
            self.files.after_edit = Some(AfterEdit::Menu {
                position,
                name,
                parent: self.path.clone(),
            });
            self.commit_rename_blur(cx);
            return;
        }
        if self.selection_busy() || self.files.editor.is_some() {
            return;
        }
        self.cancel_marquee();
        self.panel = false;
        self.files.open = false;
        if let Some(id) = id {
            if !self.selection.contains(id) {
                self.selection.select(id, false, false);
            }
        } else {
            self.selection = Selection::default();
        }
        let folder = id
            .and_then(|id| self.cache.get(&id))
            .is_some_and(|i| i.directory)
            && self.selection.count() == 1;
        let paste =
            !self.files.hub.clipboard_preparing() && operations::clipboard::Clipboard::available();
        let menu = cx.new(|cx| {
            FileMenu::new(
                self.colors,
                position,
                self.selection.count(),
                folder,
                paste,
                self.options.fixture.is_none(),
                keyboard,
                cx,
            )
        });
        self.files.menu_generation = self.generation;
        self.files.menu_subscription = Some(cx.subscribe_in(
            &menu,
            window,
            |this, source, event: &MenuEvent, window, cx| {
                if this.files.menu.as_ref().map(|m| m.entity_id()) != Some(source.entity_id()) {
                    return;
                }
                crate::popup_menu::trace("file menu event");
                let valid = this.files.menu_generation == this.generation;
                this.files.menu = None;
                window.focus(&this.focus, cx);
                if valid && let MenuEvent::Command(command) = event {
                    let command = *command;
                    this.defer_menu_action(window, cx, move |this, window, cx| {
                        this.file_command(command, window, cx)
                    });
                }
                cx.notify();
            },
        ));
        if self.options.fixture.is_none() && Self::native_owner(window) != 0 {
            let session = self
                .source
                .freeze_selection(self.selection.mask.clone())
                .ok_or_else(|| "当前列表尚未就绪".to_string())
                .and_then(|selection| {
                    crate::shell_menu::Session::new(
                        selection,
                        self.options.state_dir.clone(),
                        Self::native_owner(window),
                        self.tx.clone(),
                    )
                });
            menu.update(cx, |m, cx| m.attach_shell(session, cx));
        }
        self.files.focus_grid = false;
        let (owned, released) = crate::menu::OwnedMenu::new(menu.clone());
        self.files.menu = Some(owned);
        if let Err(error) =
            crate::popup_menu::open(menu, released, window, cx.entity().downgrade(), cx)
        {
            self.files.menu = None;
            self.navigation_error(error.to_string(), cx);
        }
        cx.notify();
    }
    /// Let the owned popup close before activating an editor in the owner.
    pub fn defer_menu_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let owner = cx.entity().downgrade();
        let generation = self.generation;
        crate::popup_menu::trace(&format!("defer menu action g={generation}"));
        window.defer(cx, move |window, cx| {
            let _ = owner.update(cx, |this, cx| {
                crate::popup_menu::trace(&format!("run menu action g={} captured={generation} files={} nav={} stale={} count={} active={}",this.generation,this.files.menu.is_some(),this.navigation.menu.is_some(),this.watch_stale(),this.selection.count(),window.is_window_active()));
                if generation != this.generation
                    || this.files.menu.is_some()
                    || this.navigation.menu.is_some()
                {
                    return;
                }
                window.activate_window();
                action(this, window, cx);
                cx.notify();
            });
        });
    }
    pub fn file_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection_busy() || self.files.editor.is_some() {
            return;
        }
        if self.watch_stale() && !matches!(command, Command::Refresh | Command::EditPath) {
            self.mark_directory_dirty(cx);
            self.navigation_error("目录正在更新，请稍后重试".into(), cx);
            return;
        }
        if crate::locations::path_copy_pending()
            && matches!(
                command,
                Command::Copy | Command::Cut | Command::Paste | Command::CopyPath
            )
        {
            self.navigation_error("正在复制路径，请稍后重试".into(), cx);
            return;
        }
        self.files.menu = None;
        self.error = None;
        if self.options.fixture.is_some() && command != Command::Details {
            return;
        }
        let owner = Self::native_owner(window);
        let count = self.selection.count();
        let id = self
            .selection
            .focus
            .filter(|id| self.selection.contains(*id));
        let result: Result<Option<u64>, String> = (|| match command {
            Command::EditPath => {
                self.edit_path(window, cx);
                Ok(None)
            }
            Command::Refresh => {
                if let Some(worker) = &self.navigation.volume_worker {
                    worker.refresh();
                }
                self.refresh_directory(None, false, cx);
                Ok(None)
            }
            Command::Open | Command::Details | Command::Rename => {
                if count != 1 {
                    return Ok(None);
                }
                if let Some(id) = id {
                    self.files.detail_intent = match command {
                        Command::Open => DetailIntent::Open,
                        Command::Rename => DetailIntent::Rename,
                        _ => DetailIntent::Show,
                    };
                    self.request_detail(id, command == Command::Open, cx);
                }
                Ok(None)
            }
            Command::Copy | Command::Cut | Command::CopyPath | Command::Recycle => {
                if count == 0 {
                    return Ok(None);
                }
                let selection = self
                    .source
                    .freeze_selection(self.selection.mask.clone())
                    .ok_or("目录尚未准备好")?;
                let id = if command == Command::Recycle {
                    self.files.hub.selection(
                        selection,
                        Operation::Recycle,
                        PathBuf::from("回收站"),
                        owner,
                    )
                } else {
                    self.files.hub.clipboard(
                        selection,
                        command == Command::Cut,
                        command == Command::CopyPath,
                    )
                }
                .map_err(|e| e.to_string())?;
                Ok(Some(id))
            }
            Command::Paste => {
                if count == 1
                    && id
                        .and_then(|id| self.cache.get(&id))
                        .is_some_and(|i| i.directory)
                {
                    self.files.detail_intent = DetailIntent::PasteFolder;
                    self.request_detail(id.unwrap(), false, cx);
                    Ok(None)
                } else {
                    self.files
                        .hub
                        .paste(
                            self.source.parent().unwrap_or_else(|| self.path.clone()),
                            self.source.parent_identity(),
                            owner,
                        )
                        .map(Some)
                        .map_err(|e| e.to_string())
                }
            }
            Command::Extension(_, _) => Ok(None),
            Command::Compress | Command::Extract => {
                self.begin_archive(
                    if command == Command::Compress {
                        crate::operations::archive::Kind::CreateZip
                    } else {
                        crate::operations::archive::Kind::Extract
                    },
                    window,
                    cx,
                );
                Ok(None)
            }
            Command::PinQuickAccess => {
                if let Some(item) = id
                    .and_then(|id| self.cache.get(&id))
                    .filter(|i| i.directory)
                {
                    self.navigation
                        .store
                        .send(crate::quick_access::Action::Pin {
                            path: self.path.join(&item.os_name),
                            title: item.name.clone(),
                            before: None,
                            revision: self.navigation.config_revision,
                        })?;
                }
                Ok(None)
            }
            Command::RemoveQuickAccess
            | Command::RenameQuickView
            | Command::MoveQuickUp
            | Command::MoveQuickDown => Ok(None),
            Command::NewFolder => {
                let id = self
                    .files
                    .hub
                    .direct(
                        PathBuf::new(),
                        self.source.parent().unwrap_or_else(|| self.path.clone()),
                        "新建文件夹".into(),
                        self.source.parent_identity(),
                        true,
                        owner,
                    )
                    .map_err(|e| e.to_string())?;
                self.files.new_folder = Some(id);
                Ok(Some(id))
            }
        })();
        match result {
            Ok(Some(id)) => self.show_operation(id),
            Ok(None) => (),
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
    pub fn show_operation(&mut self, id: u64) {
        self.files.selected = Some(id);
        self.files.expanded = false;
        self.files.failed_only = true;
        self.files.page_after = 0;
        self.files.page_history.clear();
        self.files.page_requested = None;
        self.request_operation_page();
    }
    pub fn visible_operations(&self) -> Vec<View> {
        self.files
            .hub
            .views()
            .into_iter()
            .filter(|v| !matches!(v.phase, Phase::Complete | Phase::Canceled))
            .collect()
    }
    pub fn begin_rename(
        &mut self,
        detail: Detail,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if detail.path.file_name().and_then(|n| n.to_str()).is_none() {
            self.error = Some("此名称包含无法编辑的字符，可复制完整路径处理".into());
            return;
        }
        let selected = operations::windows::names::stem_range(&detail.name, detail.directory);
        self.start_editor(
            detail.name.clone(),
            selected,
            EditKind::Rename(detail),
            window,
            cx,
        );
    }
    fn start_editor(
        &mut self,
        value: String,
        selected: Range<usize>,
        kind: EditKind,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.reset_typeahead();
        let is_path = matches!(kind, EditKind::Path);
        let input = cx.new(|cx| {
            TextInput::new(
                value,
                selected,
                if is_path { "位置" } else { "文件名称" },
                self.colors,
                if is_path { 32767 } else { 1024 },
                cx,
            )
        });
        self.files.editor_subscription =
            Some(cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                match event {
                    InputEvent::Blur(value) => {
                        if this.rename_editing() {
                            if this.files.ime_completion_inflight {
                                return;
                            }
                            if let Some(editor) = &mut this.files.editor {
                                editor.blurred = true;
                            }
                            if this.files.after_edit.is_none() {
                                let edited = this.files.editor.as_ref().and_then(|e| {
                                    if let EditKind::Rename(d) = &e.kind {
                                        Some(d.id)
                                    } else {
                                        None
                                    }
                                });
                                let selected = this
                                    .selection
                                    .focus
                                    .filter(|id| this.selection.contains(*id));
                                if selected != edited {
                                    let name = selected.and_then(|id| {
                                        this.cache.get(&id).map(|i| i.os_name.clone())
                                    });
                                    this.files.after_edit = Some(AfterEdit::Select {
                                        name,
                                        parent: this.path.clone(),
                                    });
                                }
                            }
                            if this
                                .files
                                .editor
                                .as_ref()
                                .is_some_and(|e| e.input.read(cx).is_composing())
                            {
                                this.commit_rename_blur(cx);
                            } else {
                                this.submit_editor(value.clone(), cx);
                            }
                            this.restore_rename_error_focus();
                        } else if this
                            .files
                            .editor
                            .as_ref()
                            .is_some_and(|e| matches!(e.kind, EditKind::Path))
                        {
                            this.commit_rename_blur(cx);
                        }
                    }
                    InputEvent::Cancel => {
                        if this
                            .files
                            .editor
                            .as_ref()
                            .is_some_and(|e| e.pending.is_none())
                        {
                            this.files.editor = None;
                            this.files.after_edit = None;
                            this.files.editor_subscription = None;
                            this.files.focus_grid = true;
                        }
                    }
                    InputEvent::Error(error) => {
                        if let Some(editor) = &mut this.files.editor {
                            editor.error = Some(error.clone());
                            editor.focus = true;
                        }
                        this.files.after_edit = None;
                    }
                    InputEvent::Changed => {
                        if let Some(editor) = &mut this.files.editor {
                            editor.error = None;
                        }
                    }
                    InputEvent::Submit(value) => this.submit_editor(value.clone(), cx),
                }
                cx.notify();
            }));
        self.files.menu = None;
        self.navigation.menu = None;
        self.panel = false;
        self.files.open = false;
        let focus = window.is_none();
        if let Some(window) = window {
            window.focus(&input.focus_handle(cx), cx);
        }
        self.files.editor = Some(Editor {
            input,
            kind,
            error: None,
            pending: None,
            resolving: false,
            token: 0,
            focus,
            blurred: false,
        });
        self.files.focus_grid = false;
        cx.notify();
    }
    pub fn edit_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_path_at(self.path.clone(), window, cx);
    }
    pub fn edit_path_at(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation.pending = None;
        if self.rename_editing() {
            self.files.after_edit = Some(AfterEdit::EditPathAt(path));
            self.commit_rename_blur(cx);
            return;
        }
        self.close_search();
        self.close_path_tree();
        if self.options.fixture.is_some() {
            return;
        }
        if let Some(editor) = &self.files.editor
            && matches!(editor.kind, EditKind::Path)
        {
            editor.input.update(cx, |i, cx| i.select_full(cx));
            window.focus(&editor.input.focus_handle(cx), cx);
            return;
        }
        if self
            .files
            .editor
            .as_ref()
            .is_some_and(|e| e.pending.is_some())
        {
            return;
        }
        let value = navigation::display_path(&path);
        let len = value.len();
        self.start_editor(value, 0..len, EditKind::Path, Some(window), cx);
    }
    fn submit_editor(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(editor) = self.files.editor.as_mut() else {
            return;
        };
        if editor.pending.is_some() || editor.resolving {
            return;
        }
        match &editor.kind {
            EditKind::Path => {
                if value == navigation::display_path(&self.path) {
                    self.files.editor = None;
                    self.files.editor_subscription = None;
                    self.files.focus_grid = true;
                    return;
                }
                self.detail_token += 1;
                editor.token = self.detail_token;
                editor.resolving = true;
                editor.error = None;
                let token = editor.token;
                let base = self.path.clone();
                editor.input.update(cx, |input, cx| {
                    input.enabled = false;
                    cx.notify();
                });
                self.navigation.resolver.resolve(token, base, value);
            }
            EditKind::Rename(detail) => {
                if let Err(error) = operations::windows::names::validate(&value) {
                    editor.error = Some(error.into());
                    return;
                }
                if value == detail.name {
                    self.files.editor = None;
                    self.files.editor_subscription = None;
                    self.files.focus_grid = true;
                    return;
                }
                let expected = detail.identity.as_ref().and_then(|i| Identity::decode(i));
                if expected.is_none() {
                    editor.error = Some("文件已改变，请重新选择".into());
                    return;
                }
                // HWND is captured at the window's creation; no filesystem work occurs here.
                match self.files.hub.direct(
                    detail.path.clone(),
                    detail.path.parent().unwrap_or(&self.path).to_path_buf(),
                    value.into(),
                    expected,
                    false,
                    self.native_owner_handle,
                ) {
                    Ok(id) => {
                        editor.pending = Some(id);
                        editor.error = None;
                        editor.input.update(cx, |input, cx| {
                            input.enabled = false;
                            cx.notify();
                        });
                    }
                    Err(error) => editor.error = Some(error.to_string()),
                }
            }
        }
    }
    pub fn accept_operation_detail(
        &mut self,
        detail: Detail,
        cx: &mut Context<Self>,
    ) -> Option<Detail> {
        match self.files.detail_intent {
            DetailIntent::Rename => {
                self.files.rename_location = None;
                self.files.detail_intent = DetailIntent::Show;
                self.begin_rename(detail, None, cx);
                None
            }
            DetailIntent::PasteFolder => {
                self.files.detail_intent = DetailIntent::Show;
                if detail.directory {
                    match self.files.hub.paste(
                        detail.path,
                        detail.identity.as_ref().and_then(|i| Identity::decode(i)),
                        self.native_owner_handle,
                    ) {
                        Ok(id) => self.show_operation(id),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                cx.notify();
                None
            }
            _ => Some(detail),
        }
    }
    pub fn receive_operation(&mut self, event: &Event, cx: &mut Context<Self>) -> bool {
        match event {
            Event::ClipboardChanged => {
                self.refresh_cut(cx);
                true
            }
            Event::Located {
                generation,
                token,
                id,
            } if *generation == self.generation && *token == self.files.locate_token => {
                if self.files.locate_rename && self.watch_stale() {
                    if let Some((parent, name)) = &self.files.rename_location
                        && same_location(parent, &self.path)
                    {
                        self.refresh_directory(Some(name.clone()), true, cx);
                    }
                    return true;
                }
                if self
                    .files
                    .menu_after_locate
                    .as_ref()
                    .is_some_and(|(awaited, _, _)| awaited == token)
                {
                    let (_, position, parent) = self.files.menu_after_locate.take().unwrap();
                    if same_location(&parent, &self.path) {
                        if let Some(id) = id {
                            self.files.ready_menu = Some((position, *id));
                        } else {
                            self.error = Some("文件已不在此位置".into());
                        }
                    }
                }
                if let Some(id) = id {
                    self.selection.select(*id, false, false);
                    self.scroll_global(*id / self.columns, ScrollStrategy::Nearest, false);
                    self.files.focus_grid = true;
                    if self.files.locate_rename {
                        self.files.locate_rename = false;
                        self.files.detail_intent = DetailIntent::Rename;
                        self.request_detail(*id, false, cx);
                    }
                }
                cx.notify();
                true
            }
            Event::PathResolved { token, value }
                if self
                    .files
                    .editor
                    .as_ref()
                    .is_some_and(|e| matches!(e.kind, EditKind::Path) && e.token == *token) =>
            {
                match value {
                    Ok(path) => {
                        self.files.editor = None;
                        self.files.editor_subscription = None;
                        self.navigate(path.clone(), true, cx);
                        self.files.focus_grid = true;
                    }
                    Err(error) => {
                        self.files.after_edit = None;
                        if let Some(editor) = &mut self.files.editor {
                            editor.resolving = false;
                            editor.error = Some(error.clone());
                            editor.focus = true;
                            editor.input.update(cx, |i, cx| {
                                i.enabled = true;
                                cx.notify();
                            });
                        }
                    }
                }
                cx.notify();
                true
            }
            Event::PathResolved { token, value } if self.navigation.pending == Some(*token) => {
                self.navigation.pending = None;
                match value {
                    Ok(path) => {
                        self.navigate(path.clone(), true, cx);
                        self.files.focus_grid = true;
                    }
                    Err(error) => self.navigation_error(error.clone(), cx),
                }
                cx.notify();
                true
            }
            _ => false,
        }
    }
    pub fn file_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if self.files.close_guard {
            if key == "escape" && !self.files.exit_after_cancel {
                self.files.close_guard = false;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            return key != "tab";
        }
        if m.platform {
            return false;
        }
        if (m.control && !m.alt && key == "l") || (m.alt && !m.control && key == "d") {
            self.edit_path(window, cx);
            return true;
        }
        if self.files.menu.is_some() {
            return true;
        }
        if self.files.editor.is_some() {
            return false;
        }
        if !self.focus.is_focused(window) {
            return false;
        }
        if key == "escape" && self.files.open {
            self.files.open = false;
            cx.notify();
            return true;
        }
        if key == "escape" && self.files.hub.own_clipboard().is_some_and(|c| c.cut) {
            if let Err(e) = self.files.hub.clear_cut() {
                self.error = Some(e.to_string());
            }
            cx.notify();
            return true;
        }
        if !m.alt && !m.control && (key == "menu" || (key == "f10" && m.shift)) {
            let id = self.selection.focus;
            let anchor = id
                .map(|id| {
                    let col = id % self.columns;
                    let row = id / self.columns;
                    let top = self
                        .global_point(point(px(12.), px(layout::content_top())))
                        .1;
                    point(
                        px(layout::content_left(self.sidebar_width())
                            + col as f32 * (self.cell_width + layout::LAYOUT.grid.column_gap)),
                        px((f64::from(layout::content_top())
                            + row as f64 * layout::row_step() as f64
                            - top
                            + layout::LAYOUT.grid.cell_height as f64)
                            as f32),
                    )
                })
                .unwrap_or(point(px(self.sidebar_width() + 20.), px(68.)));
            self.open_menu(anchor, id, true, window, cx);
            return true;
        }
        let command = match (key, m.control, m.shift, m.alt) {
            ("c", true, true, false) => Some(Command::CopyPath),
            ("c", true, false, false) => Some(Command::Copy),
            ("x", true, false, false) => Some(Command::Cut),
            ("v", true, false, false) => Some(Command::Paste),
            ("d", true, false, false) => Some(Command::Recycle),
            ("n", true, true, false) => Some(Command::NewFolder),
            ("f2", false, false, false) => Some(Command::Rename),
            ("f5", false, false, false) => Some(Command::Refresh),
            ("delete", false, false, false) => Some(Command::Recycle),
            ("enter", false, false, true) => Some(Command::Details),
            _ => None,
        };
        if let Some(command) = command {
            if !event.is_held {
                self.file_command(command, window, cx);
            }
            return true;
        }
        false
    }
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ole.as_ref().is_some_and(|site| site.busy()) {
            self.navigation_error("请先完成或取消拖放".into(), cx);
            return;
        }
        if !self.flush_view_fields(window, cx) || !self.flush_archive_name(cx) {
            return;
        }
        if let Some(input) = &self.search.input
            && (input.read(cx).is_composing() || input.read(cx).is_pasting())
        {
            self.restore.close_requested = true;
            window.focus(&input.focus_handle(cx), cx);
            self.navigation_error("请先完成输入".into(), cx);
            return;
        }
        if self.files.editor.is_some() {
            self.files.after_edit = Some(AfterEdit::Close);
            self.commit_rename_blur(cx);
            return;
        }
        self.close_path_tree();
        if self.files.hub.active()
            && cx
                .windows()
                .iter()
                .filter(|w| w.downcast::<Backleaf>().is_some())
                .count()
                <= 1
        {
            self.files.close_guard = true;
            self.files.open = false;
            self.files.menu = None;
            cx.notify();
        } else {
            self.close_window_saved(window, cx);
        }
    }
    pub fn sync_operation_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.refresh_pending && self.files.editor.is_none() {
            self.files.refresh_pending = false;
            self.refresh_directory(None, false, cx);
        }
        self.files
            .hub
            .set_visible(window.is_visible() && !self.session_locked);
        if self.files.exit_after_cancel && !self.files.hub.active() {
            self.close_window_saved(window, cx);
            return;
        }
        if self.files.editor.is_none()
            && let Some(action) = self.files.after_edit.take()
        {
            match action {
                AfterEdit::Navigate(path, push) => self.navigate(path, push, cx),
                AfterEdit::Close => self.request_close(window, cx),
                AfterEdit::Search => {
                    self.open_search(window, cx);
                    return;
                }
                AfterEdit::Location(path) => {
                    self.request_location(path, cx);
                    return;
                }
                AfterEdit::EditPathAt(path) => {
                    self.edit_path_at(path, window, cx);
                    return;
                }
                AfterEdit::BreadcrumbMenu {
                    path,
                    position,
                    keyboard,
                } => {
                    self.show_crumb_menu(path, position, keyboard, window, cx);
                    return;
                }
                AfterEdit::Archive(kind) => self.begin_archive(kind, window, cx),
                AfterEdit::QuickMenu(entry, position, keyboard) => {
                    self.show_quick_menu(entry, position, keyboard, window, cx)
                }
                AfterEdit::SavedView(entry, view) => self.open_saved_view(entry, view, window, cx),
                AfterEdit::PathTree(ancestors) => {
                    if ancestors {
                        self.show_ancestor_menu(window, cx);
                    } else {
                        self.navigation.rows.clear();
                        self.show_path_tree(true, Some(window), cx);
                    }
                    return;
                }
                AfterEdit::Menu {
                    position,
                    name,
                    parent,
                } => {
                    if same_location(&parent, &self.path) {
                        if let Some(name) = name {
                            self.files.locate_token += 1;
                            self.files.locate_rename = false;
                            self.files.menu_after_locate =
                                Some((self.files.locate_token, position, parent));
                            self.source.locate(self.files.locate_token, name);
                        } else {
                            self.files.locate_token += 1;
                            self.open_menu(position, None, false, window, cx);
                            return;
                        }
                    }
                }
                AfterEdit::Select { name, parent } => {
                    if same_location(&parent, &self.path) {
                        if let Some(name) = name {
                            self.files.locate_token += 1;
                            self.files.locate_rename = false;
                            self.source.locate(self.files.locate_token, name);
                        } else {
                            self.files.locate_token += 1;
                            self.selection = Selection::default();
                        }
                    }
                }
            }
        }
        if let Some((position, id)) = self.files.ready_menu
            && self.cache.contains_key(&id)
        {
            self.files.ready_menu = None;
            self.open_menu(position, Some(id), false, window, cx);
            return;
        }
        if let Some(editor) = &mut self.files.editor
            && editor.focus
        {
            self.files.focus_grid = false;
            editor.focus = false;
            window.focus(&editor.input.focus_handle(cx), cx);
        } else if self.files.focus_grid {
            self.files.focus_grid = false;
            window.focus(&self.focus, cx);
        }
    }
    pub fn rename_editing(&self) -> bool {
        self.files
            .editor
            .as_ref()
            .is_some_and(|e| matches!(e.kind, EditKind::Rename(_)))
    }
    fn restore_rename_error_focus(&mut self) {
        if let Some(editor) = &mut self.files.editor
            && editor.pending.is_none()
            && editor.error.is_some()
        {
            editor.focus = true;
            self.files.after_edit = None;
        }
    }
    pub fn commit_rename_blur(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self
            .files
            .editor
            .as_ref()
            .filter(|e| e.pending.is_none() && !e.resolving)
        else {
            return;
        };
        if editor.input.read(cx).is_pasting() {
            editor
                .input
                .update(cx, |input, _| input.wait_for_composition_blur());
            if let Some(editor) = &mut self.files.editor {
                editor.blurred = true;
            }
            return;
        }
        if editor.input.read(cx).is_composing() {
            let input = editor.input.clone();
            input.update(cx, |input, _| input.wait_for_composition_blur());
            if let Some(editor) = &mut self.files.editor {
                editor.focus = true;
                editor.blurred = true;
            }
            if !self.files.ime_completion_inflight {
                self.files.ime_completion_inflight = true;
                let owner = self.native_owner_handle;
                let input_id = input.entity_id();
                let before = input.read(cx).value();
                let without = input.read(cx).without_composition();
                cx.spawn(async move |this, cx| {
                    let current = this
                        .update(cx, |this, cx| {
                            this.files.editor.as_ref().is_some_and(|e| {
                                e.input.entity_id() == input_id && e.input.read(cx).is_composing()
                            })
                        })
                        .unwrap_or(false);
                    // IMM can reenter GPUI. Call it outside App/Window::update, with
                    // the original editor retained and its focus restored for this frame.
                    let completed = current && crate::input::complete_native_composition(owner);
                    cx.background_executor()
                        .timer(Duration::from_millis(80))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.files.ime_completion_inflight = false;
                        let Some(editor) = this
                            .files
                            .editor
                            .as_mut()
                            .filter(|e| e.input.entity_id() == input_id && e.pending.is_none())
                        else {
                            return;
                        };
                        let discarded = without.as_ref().is_some_and(|without| {
                            *without == editor.input.read(cx).value() && *without != before
                        });
                        if !completed || editor.input.read(cx).is_composing() || discarded {
                            if discarded {
                                editor.input.update(cx, |input, cx| {
                                    input.restore_unconfirmed_composition(before, cx)
                                });
                            }
                            editor.error = Some("请先完成输入法候选，再重试".into());
                            editor.focus = true;
                            this.files.after_edit = None;
                        } else {
                            this.commit_rename_blur(cx);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
            cx.notify();
            return;
        }
        let value = editor.input.read(cx).value();
        if let Some(editor) = &mut self.files.editor {
            editor.blurred = true;
        }
        self.submit_editor(value, cx);
        self.restore_rename_error_focus();
        cx.notify();
    }
    pub fn defer_archive(
        &mut self,
        kind: crate::operations::archive::Kind,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::Archive(kind));
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_quick_menu(
        &mut self,
        entry: crate::quick_access::Entry,
        position: Point<Pixels>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::QuickMenu(entry, position, keyboard));
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_saved_view(
        &mut self,
        entry: crate::quick_access::Entry,
        view: crate::file_view::FileView,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::SavedView(entry, view));
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_navigation(&mut self, path: PathBuf, push: bool, cx: &mut Context<Self>) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::Navigate(path, push));
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_search(&mut self, cx: &mut Context<Self>) -> bool {
        if self.files.editor.is_some() {
            self.files.after_edit = Some(AfterEdit::Search);
            self.commit_rename_blur(cx);
            true
        } else {
            self.files.editor = None;
            self.files.editor_subscription = None;
            false
        }
    }
    pub fn commit_rename_selection(&mut self, cx: &mut Context<Self>) {
        let name = self
            .selection
            .focus
            .filter(|id| self.selection.contains(*id))
            .and_then(|id| self.cache.get(&id).map(|item| item.os_name.clone()));
        self.files.after_edit = Some(AfterEdit::Select {
            name,
            parent: self.path.clone(),
        });
        self.commit_rename_blur(cx);
    }
    pub fn defer_path_tree(&mut self, ancestors: bool, cx: &mut Context<Self>) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::PathTree(ancestors));
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_crumb_menu(
        &mut self,
        path: PathBuf,
        position: Point<Pixels>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::BreadcrumbMenu {
            path,
            position,
            keyboard,
        });
        self.commit_rename_blur(cx);
        true
    }
    pub fn defer_location(&mut self, path: PathBuf, cx: &mut Context<Self>) -> bool {
        if self.files.editor.is_none() {
            return false;
        }
        self.files.after_edit = Some(AfterEdit::Location(path));
        self.commit_rename_blur(cx);
        true
    }
    fn request_operation_page(&mut self) {
        if !self.files.open || !self.files.expanded {
            return;
        }
        if let Some(id) = self.files.selected
            && let Some(v) = self.files.hub.view(id)
        {
            if self.files.failed_only && v.failed == 0 && v.unknown == 0 {
                return;
            }
            let signature = (
                id,
                self.files.page_after,
                self.files.failed_only,
                v.successful + v.failed + v.unknown,
                v.phase.terminal(),
            );
            if self.files.page_requested != Some(signature) {
                self.files.page_requested = Some(signature);
                self.files
                    .hub
                    .page(id, self.files.page_after, self.files.failed_only);
            }
        }
    }
    fn text_action(
        &self,
        id: &'static str,
        label: &'static str,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let colors = self.colors;
        let action = Rc::new(click);
        let keyboard = action.clone();
        div()
            .id(id)
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
            .focus_visible(move |d| {
                d.bg(colors.color(ColorRole::InputActive))
                    .shadow(colors.control_shadows(true))
            })
            .on_key_down(move |e, w, cx| {
                if paint::activation_key(e) {
                    cx.stop_propagation();
                    keyboard(&ClickEvent::default(), w, cx);
                }
            })
            .on_click(move |e, w, cx| action(e, w, cx))
            .child(label)
    }
    pub fn operation_entry(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.visible_operations().len();
        let colors = self.colors;
        div()
            .id("operations-entry")
            .h(px(32.))
            .px(px(8.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .role(Role::Button)
            .aria_label(format!("文件操作，{count} 项任务"))
            .tooltip(tooltip(format!("文件操作 · {count} 项任务"), colors))
            .focusable()
            .tab_stop(true)
            .hover(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(false))
            })
            .focus(move |d| {
                d.bg(colors.color(ColorRole::Floating))
                    .shadow(colors.control_shadows(true))
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.files.open = !this.files.open;
                let views = this.visible_operations();
                this.files.selected = (views.len() == 1).then(|| views[0].id);
                this.files.expanded = true;
                this.files.failed_only = true;
                this.request_operation_page();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if paint::activation_key(e) {
                    this.files.open = !this.files.open;
                    let views = this.visible_operations();
                    this.files.selected = (views.len() == 1).then(|| views[0].id);
                    this.files.expanded = true;
                    this.files.failed_only = true;
                    this.request_operation_page();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(icon("more", colors))
            .child(number(count))
            .into_any_element()
    }
    pub fn operation_overlay(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let mut layer = div().absolute().inset_0();
        if self.files.open {
            let content =
                if let Some(view) = self.files.selected.and_then(|id| self.files.hub.view(id)) {
                    self.operation_detail(view, window, cx)
                } else {
                    self.operation_queue(cx)
                };
            layer = layer.child(
                div()
                    .id("operation-detail-overlay")
                    .absolute()
                    .right(px(12.))
                    .bottom(px(36.))
                    .w(px(320.))
                    .max_w(px((f32::from(window.viewport_size().width) - 24.).max(1.)))
                    .child(content),
            );
        }
        if let Some(editor) = &self.files.editor
            && let Some(error) = &editor.error
        {
            let (x, y) = match &editor.kind {
                EditKind::Path => (80., layout::CHROME.bar_height),
                EditKind::Rename(d) => {
                    let top = self
                        .global_point(point(px(12.), px(layout::content_top())))
                        .1;
                    (
                        layout::content_left(self.sidebar_width())
                            + (d.id % self.columns) as f32
                                * (self.cell_width + layout::LAYOUT.grid.column_gap),
                        (f64::from(layout::content_top())
                            + (d.id / self.columns) as f64 * layout::row_step() as f64
                            - top
                            + 112.) as f32,
                    )
                }
            };
            layer = layer.child(
                div()
                    .id("editor-error-toast")
                    .absolute()
                    .left(px(x.clamp(
                        8.,
                        (f32::from(window.viewport_size().width) - 288.).max(8.),
                    )))
                    .top(px(y.clamp(
                        layout::CHROME.bar_height,
                        (f32::from(window.viewport_size().height) - 60.)
                            .max(layout::CHROME.bar_height),
                    )))
                    .w(px(280.))
                    .px(px(8.))
                    .py(px(12.))
                    .rounded(px(8.))
                    .bg(colors.color(ColorRole::Floating))
                    .border_1()
                    .border_color(colors.color(ColorRole::Border))
                    .shadow(vec![colors.shadow()])
                    .text_size(px(12.))
                    .text_color(colors.color(ColorRole::Negative))
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .child(error.clone()),
            );
        }
        if self.files.close_guard {
            let count = self
                .files
                .hub
                .views()
                .iter()
                .filter(|v| !v.phase.terminal())
                .count();
            layer = layer.child(
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
                            .w(px(400.))
                            .px(px(12.))
                            .py(px(16.))
                            .rounded(px(12.))
                            .bg(colors.color(ColorRole::Floating))
                            .border_1()
                            .border_color(colors.color(ColorRole::Border))
                            .shadow(vec![colors.shadow()])
                            .child(format!("还有 {count} 项任务"))
                            .child(
                                div()
                                    .mt(px(24.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(self.text_action(
                                        "continue-operations",
                                        "继续处理",
                                        cx.listener(|this, _, window, cx| {
                                            this.files.close_guard = false;
                                            window.focus(&this.focus, cx);
                                            cx.notify();
                                        }),
                                    ))
                                    .child(self.text_action(
                                        "cancel-and-exit",
                                        if self.files.exit_after_cancel {
                                            "取消中"
                                        } else {
                                            "取消任务并退出"
                                        },
                                        cx.listener(|this, _, _, cx| {
                                            this.files.exit_after_cancel = true;
                                            this.files.hub.cancel_all();
                                            cx.notify();
                                        }),
                                    )),
                            ),
                    ),
            );
        }
        layer.into_any_element()
    }
    fn panel_shell(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = self.colors;
        div()
            .id(id)
            .occlude()
            .w_full()
            .px(px(12.))
            .py(px(16.))
            .rounded(px(12.))
            .bg(colors.color(ColorRole::Floating))
            .border_1()
            .border_color(colors.color(ColorRole::Border))
            .shadow(vec![colors.shadow()])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, e: &MouseDownEvent, _, cx| {
                if e.button == MouseButton::Left {
                    this.files.open = false;
                    cx.notify();
                }
            }))
    }
    fn operation_queue(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let views = self.visible_operations();
        self.panel_shell("operation-queue", cx)
            .child(
                div()
                    .id("operation-queue-scroll")
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .children(views.into_iter().rev().map(|v| {
                        let id = v.id;
                        let label = format!(
                            "{} → {}",
                            v.operation.label(),
                            v.target
                                .file_name()
                                .unwrap_or(v.target.as_os_str())
                                .to_string_lossy()
                        );
                        div()
                            .id(("queued-task", id))
                            .w_full()
                            .py(px(12.))
                            .px(px(4.))
                            .rounded(px(8.))
                            .role(Role::Button)
                            .aria_label(format!("{label}，{}", phase_label(v.phase)))
                            .focusable()
                            .tab_stop(true)
                            .hover(move |d| d.bg(colors.color(ColorRole::Hover)))
                            .focus_visible(move |d| {
                                d.bg(colors.color(ColorRole::InputActive))
                                    .shadow(colors.control_shadows(true))
                            })
                            .tooltip(tooltip(
                                format!("{label}\n{}", phase_label(v.phase)),
                                colors,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.show_operation(id);
                                this.files.open = true;
                                this.files.expanded = true;
                                this.request_operation_page();
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                                if paint::activation_key(e) {
                                    this.show_operation(id);
                                    this.files.open = true;
                                    this.files.expanded = true;
                                    this.request_operation_page();
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }))
                            .child(div().truncate().child(label))
                            .child(
                                div()
                                    .h(px(2.))
                                    .mt(px(8.))
                                    .bg(colors.color(ColorRole::Border))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(fraction(&v)))
                                            .bg(colors.color(ColorRole::Accent)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .mt(px(8.))
                                    .text_size(px(12.))
                                    .text_color(colors.color(ColorRole::Secondary))
                                    .child(operation_summary(&v)),
                            )
                    })),
            )
            .into_any_element()
    }
    fn operation_detail(&self, v: View, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let id = v.id;
        let terminal = v.phase.terminal();
        let target = v.target.to_string_lossy().into_owned();
        let destination = v
            .target
            .file_name()
            .unwrap_or(v.target.as_os_str())
            .to_string_lossy();
        let failures = v.failed.saturating_add(v.unknown);
        let title = if terminal {
            format!("{} 项未完成", number(failures.max(1) as usize))
        } else {
            format!("{} → {destination}", v.operation.label())
        };
        let action = self.button(
            "close",
            if terminal {
                "关闭任务结果"
            } else if v.phase == Phase::Canceling {
                "正在取消"
            } else {
                "取消任务"
            },
            cx.listener(move |this, _, window, cx| {
                if terminal {
                    this.files.hub.dismiss(id);
                    this.files.selected = None;
                    this.files.open = false;
                    window.focus(&this.focus, cx);
                } else {
                    this.files.hub.cancel(id);
                }
                cx.notify();
            }),
        );
        let mut panel = self
            .panel_shell("real-operation", cx)
            .role(Role::Group)
            .aria_label(format!("{}，{}", v.operation.label(), phase_label(v.phase)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(28.))
                    .child(
                        div()
                            .id("operation-caption")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .tooltip(tooltip(
                                format!("{} → {target}", v.operation.label()),
                                colors,
                            ))
                            .child(title),
                    )
                    .child(action),
            )
            .child(
                div()
                    .id("operation-progress")
                    .mt(px(8.))
                    .h(px(2.))
                    .bg(colors.color(ColorRole::Border))
                    .role(Role::ProgressIndicator)
                    .aria_label("当前进度")
                    .aria_numeric_value(fraction(&v) as f64 * 100.)
                    .aria_min_numeric_value(0.)
                    .aria_max_numeric_value(100.)
                    .child(
                        div()
                            .h_full()
                            .w(relative(fraction(&v)))
                            .bg(colors.color(ColorRole::Accent)),
                    ),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(12.))
                    .text_color(colors.color(ColorRole::Secondary))
                    .child(operation_summary(&v)),
            );
        if !terminal && let Some(active) = v.active.first() {
            let animate = window.is_visible()
                && !self.session_locked
                && !self.options.reduced_motion
                && !cx.reduce_motion();
            panel = panel.child(
                div()
                    .id(SharedString::from(format!(
                        "operation-active-{}-{}",
                        active.key, active.token
                    )))
                    .mt(px(8.))
                    .h(px(24.))
                    .tooltip(tooltip(active.name.to_string(), colors))
                    .child(paint::flowing_name(
                        active.name.to_string(),
                        (v.started.elapsed().as_secs_f32() / 1.8).fract(),
                        animate,
                        self.flow_cache.clone(),
                        colors,
                    )),
            );
        }
        if let Some(error) = v
            .error
            .as_ref()
            .or_else(|| v.page.as_ref().and_then(|p| p.error.as_ref()))
        {
            panel = panel.child(
                div()
                    .id("operation-error")
                    .mt(px(8.))
                    .text_size(px(12.))
                    .text_color(colors.color(ColorRole::Negative))
                    .truncate()
                    .tooltip(tooltip(error.to_string(), colors))
                    .child(error.to_string()),
            );
        }
        if self.files.expanded && failures > 0 {
            let mut next = None;
            let limit = if f32::from(window.viewport_size().height) < 560. {
                4
            } else {
                8
            };
            if let Some(page) = &v.page
                && page.after == self.files.page_after
                && page.failed_only
            {
                let mut rows = Vec::new();
                for row in page.rows.iter().take(limit) {
                    next = Some(row.sequence);
                    let name = row
                        .path
                        .file_name()
                        .unwrap_or(row.path.as_os_str())
                        .to_string_lossy()
                        .into_owned();
                    let reason = row
                        .hresult
                        .filter(|n| *n < 0)
                        .map(operations::service::error_label)
                        .unwrap_or_else(|| state_label(row.state).into());
                    let full = format!("{}\n{reason}", row.path.to_string_lossy());
                    rows.push(
                        div()
                            .id(("operation-file", row.sequence))
                            .w_full()
                            .py(px(6.))
                            .role(Role::ListBoxOption)
                            .aria_label(full.clone())
                            .tooltip(tooltip(full, colors))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(colors.color(ColorRole::Negative))
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(colors.color(ColorRole::Secondary))
                                    .truncate()
                                    .child(reason),
                            ),
                    );
                }
                if page.rows.len() <= limit {
                    next = None;
                }
                panel = panel.child(div().mt(px(8.)).children(rows));
            }
            if !self.files.page_history.is_empty() || next.is_some() {
                panel = panel.child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.))
                        .when(!self.files.page_history.is_empty(), |d| {
                            d.child(self.button(
                                "back",
                                "上一页未完成项目",
                                cx.listener(|this, _, _, cx| {
                                    if let Some(after) = this.files.page_history.pop() {
                                        this.files.page_after = after;
                                        this.files.page_requested = None;
                                        this.request_operation_page();
                                        cx.notify();
                                    }
                                }),
                            ))
                        })
                        .when_some(next, |d, next| {
                            d.child(self.button(
                                "more",
                                "下一页未完成项目",
                                cx.listener(move |this, _, _, cx| {
                                    if this.files.page_history.len() >= 128 {
                                        this.files.page_history.remove(0);
                                    }
                                    this.files.page_history.push(this.files.page_after);
                                    this.files.page_after = next;
                                    this.files.page_requested = None;
                                    this.request_operation_page();
                                    cx.notify();
                                }),
                            ))
                        }),
                );
            }
        }
        panel.into_any_element()
    }
}
fn same_location(a: &std::path::Path, b: &std::path::Path) -> bool {
    fn key(p: &std::path::Path) -> String {
        let value = p.to_string_lossy().replace('/', "\\");
        let value = value.strip_prefix("\\\\?\\").unwrap_or(&value);
        let value = if let Some(unc) = value.strip_prefix("UNC\\") {
            format!("\\\\{unc}")
        } else {
            value.to_owned()
        };
        value.trim_end_matches('\\').to_lowercase()
    }
    key(a) == key(b)
}
fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Queued => "已排队",
        Phase::Preparing => "正在准备",
        Phase::Running => "进行中",
        Phase::Finishing => "正在确认结果",
        Phase::Canceling => "正在取消",
        Phase::Complete => "已完成",
        Phase::Partial => "部分完成",
        Phase::Canceled => "已取消",
        Phase::Failed => "未完成",
        Phase::Unknown => "结果尚未确认",
    }
}
fn state_label(state: State) -> &'static str {
    match state {
        State::Pending | State::Dispatched => "未处理",
        State::Active => "处理中",
        State::Provisional => "确认中",
        State::Success => "已完成",
        State::Failed => "失败",
        State::Skipped => "已跳过",
        State::Canceled => "已取消",
        State::Partial => "部分完成",
        State::Unknown => "结果尚未确认",
        State::NotProcessed => "未处理",
    }
}
fn count_label(v: &View) -> String {
    let count = if v.phase == Phase::Preparing {
        v.prepared
    } else {
        v.successful
    };
    format!(
        "{} / {} 项",
        number(count as usize),
        v.total
            .map(|n| number(n as usize))
            .unwrap_or_else(|| "—".into())
    )
}
fn operation_summary(v: &View) -> String {
    let count = count_label(v);
    let bytes = bytes_label(v);
    if bytes.is_empty() {
        count
    } else {
        format!("{count} · {bytes}")
    }
}

fn size(n: u64) -> String {
    crate::volume_capacity::bytes(u128::from(n))
}

fn bytes_label(v: &View) -> String {
    if !v.size_available {
        return String::new();
    }
    let done = size(v.bytes.saturating_add(v.in_flight));
    match v.bytes_total {
        Some(total) => format!("{done} / {}", size(total)),
        None => done,
    }
}

fn fraction(v: &View) -> f32 {
    if v.phase == Phase::Complete {
        return 1.;
    }
    if v.phase == Phase::Preparing {
        return 0.;
    }
    if let Some(total) = v.bytes_total.filter(|n| *n > 0) {
        ((v.bytes.saturating_add(v.in_flight)) as f64 / total as f64).clamp(0., 1.) as f32
    } else if let Some(total) = v.total.filter(|n| *n > 0) {
        (v.successful as f64 / total as f64).clamp(0., 1.) as f32
    } else {
        0.
    }
}

#[cfg(test)]
mod location_tests {
    use super::same_location;
    #[test]
    fn refresh_matches_normal_and_shell_path_spellings() {
        for (a, b) in [
            (r"D:/work/资料", r"\\?\D:\work\资料"),
            (r"\\server\share\文档", r"\\?\UNC\server\share\文档"),
            (r"C:\Users\Simo\", r"c:/users/simo"),
        ] {
            assert!(same_location(a.as_ref(), b.as_ref()));
        }
        assert!(!same_location(r"D:\a".as_ref(), r"D:\ab".as_ref()));
    }
}
