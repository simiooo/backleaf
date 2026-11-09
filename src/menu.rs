//! Context-specific file commands; the menu captures its context before keyboard focus moves.
use crate::{
    layout::LAYOUT,
    paint::tooltip,
    theme::{Palette, Role as ColorRole},
};
use gpui::{prelude::*, *};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Open,
    Copy,
    Cut,
    Paste,
    CopyPath,
    Rename,
    Recycle,
    Details,
    NewFolder,
    Refresh,
    EditPath,
    Compress,
    Extract,
    Extension(u32, bool),
    PinQuickAccess,
    RemoveQuickAccess,
    RenameQuickView,
    MoveQuickUp,
    MoveQuickDown,
    OpenElevated,
}
impl Command {
    fn label(self, folder: bool) -> &'static str {
        match self {
            Self::Open => crate::i18n::tr("打开"),
            Self::Copy => crate::i18n::tr("复制"),
            Self::Cut => crate::i18n::tr("剪切"),
            Self::Paste => {
                if folder {
                    crate::i18n::tr("粘贴到文件夹")
                } else {
                    crate::i18n::tr("粘贴")
                }
            }
            Self::CopyPath => crate::i18n::tr("复制路径"),
            Self::Rename => crate::i18n::tr("重命名"),
            Self::Recycle => crate::i18n::tr("移至回收站"),
            Self::Details => crate::i18n::tr("详情"),
            Self::NewFolder => crate::i18n::tr("新建文件夹"),
            Self::Refresh => crate::i18n::tr("刷新"),
            Self::EditPath => crate::i18n::tr("编辑路径"),
            Self::Compress => crate::i18n::tr("压缩为 ZIP…"),
            Self::Extract => crate::i18n::tr("解压到…"),
            Self::Extension(_, _) => "",
            Self::PinQuickAccess => crate::i18n::tr("固定到侧栏"),
            Self::RemoveQuickAccess => crate::i18n::tr("移除入口"),
            Self::RenameQuickView => crate::i18n::tr("重命名视图"),
            Self::MoveQuickUp => crate::i18n::tr("上移"),
            Self::MoveQuickDown => crate::i18n::tr("下移"),
            Self::OpenElevated => crate::i18n::tr("以管理员身份打开"),
        }
    }
    fn shortcut(self) -> &'static str {
        match self {
            Self::Open => "Enter",
            Self::Copy => "Ctrl+C",
            Self::Cut => "Ctrl+X",
            Self::Paste => "Ctrl+V",
            Self::CopyPath => "Ctrl+Shift+C",
            Self::Rename => "F2",
            Self::Recycle => "Delete",
            Self::Details => "Alt+Enter",
            Self::NewFolder => "Ctrl+Shift+N",
            Self::Refresh => "F5",
            Self::EditPath => "Ctrl+L",
            Self::Compress | Self::Extract | Self::Extension(_, _) => "",
            Self::PinQuickAccess
            | Self::RemoveQuickAccess
            | Self::RenameQuickView
            | Self::OpenElevated => "",
            Self::MoveQuickUp => "Alt+↑",
            Self::MoveQuickDown => "Alt+↓",
        }
    }
}
/// The owner holds the lease; the popup holds only its view and release receiver.
#[derive(Clone)]
pub struct OwnedMenu {
    entity: Entity<FileMenu>,
    _lease: std::sync::Arc<async_channel::Sender<()>>,
}
impl OwnedMenu {
    pub fn new(entity: Entity<FileMenu>) -> (Self, async_channel::Receiver<()>) {
        let (tx, rx) = async_channel::bounded(1);
        (
            Self {
                entity,
                _lease: std::sync::Arc::new(tx),
            },
            rx,
        )
    }
}
impl std::ops::Deref for OwnedMenu {
    type Target = Entity<FileMenu>;
    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}
pub enum MenuEvent {
    Command(Command),
    Dismiss,
}
#[derive(Clone)]
struct Entry {
    command: Command,
    label: String,
    hint: String,
    disabled: Option<String>,
    checked: bool,
    radio: bool,
}
struct Column {
    scroll: ScrollHandle,
    parent: u32,
    rows: Vec<Option<Entry>>,
    current: usize,
    token: u64,
    ready: bool,
}
#[derive(Clone, Copy)]
pub struct ColumnGeometry {
    pub token: u64,
    pub width: f32,
    pub height: f32,
    pub row_offset: f32,
}
pub struct FileMenu {
    colors: &'static Palette,
    focus: FocusHandle,
    anchor: Point<Pixels>,
    columns: Vec<Column>,
    keyboard: bool,
    breadcrumb: bool,
    active: usize,
    base: usize,
    session: Option<std::sync::Arc<crate::shell_menu::Session>>,
    pub dismissed: bool,
    hover_serial: u64,
    applications: Option<Vec<Option<Entry>>>,
    next_column: u64,
}
impl EventEmitter<MenuEvent> for FileMenu {}
impl Focusable for FileMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl FileMenu {
    fn entry(command: Command, folder: bool, disabled: Option<String>) -> Entry {
        Entry {
            command,
            label: command.label(folder).into(),
            hint: command.shortcut().into(),
            disabled,
            checked: false,
            radio: false,
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Frozen context and native popup placement are explicit"
    )]
    pub fn new(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        selected: usize,
        folder: bool,
        paste: bool,
        real: bool,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        use Command::*;
        let mut commands = if selected == 0 {
            vec![
                Some(NewFolder),
                Some(Paste),
                None,
                Some(Refresh),
                None,
                Some(OpenElevated),
            ]
        } else if selected > 1 {
            vec![
                Some(Copy),
                Some(Cut),
                Some(CopyPath),
                None,
                Some(Rename),
                Some(Recycle),
            ]
        } else {
            let mut v = vec![
                Some(Open),
                None,
                Some(Copy),
                Some(Cut),
                Some(if folder { Paste } else { CopyPath }),
                None,
                Some(Rename),
                Some(Recycle),
                None,
                Some(Details),
            ];
            if folder {
                v.push(Some(PinQuickAccess));
                v.push(Some(OpenElevated));
            }
            v
        };
        if selected > 0 {
            commands.extend([None, Some(Compress)]);
            if !folder && selected == 1 {
                commands.push(Some(Extract))
            }
        }
        let rows = commands
            .into_iter()
            .map(|command| {
                command.map(|command| {
                    Self::entry(
                        command,
                        folder,
                        if !real && command != Details {
                            Some(crate::i18n::tr_owned("渲染基准不执行文件命令"))
                        } else if command == Rename && selected > 1 {
                            Some(crate::i18n::tr_owned("一次重命名一个项目"))
                        } else if command == Paste && !paste {
                            Some(crate::i18n::tr_owned("剪贴板中没有文件"))
                        } else {
                            None
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        let mut menu = Self::with_rows(colors, anchor, folder, rows, keyboard, false, cx);
        if real {
            menu.status(0, crate::i18n::tr_owned("读取扩展…"));
        }
        menu
    }
    fn with_rows(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        folder: bool,
        rows: Vec<Option<Entry>>,
        keyboard: bool,
        breadcrumb: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = folder;
        let current = rows
            .iter()
            .position(|r| r.as_ref().is_some_and(|r| r.disabled.is_none()))
            .unwrap_or(0);
        let base = rows.len();
        Self {
            colors,
            focus: cx.focus_handle().tab_stop(true),
            anchor,
            columns: vec![Column {
                scroll: ScrollHandle::new(),
                parent: u32::MAX,
                rows,
                current,
                token: 0,
                ready: true,
            }],
            keyboard,
            breadcrumb,
            active: 0,
            base,
            session: None,
            dismissed: false,
            hover_serial: 0,
            applications: None,
            next_column: 0,
        }
    }
    pub fn breadcrumb(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut copy = Self::entry(Command::CopyPath, true, None);
        copy.label = "复制文件路径".into();
        copy.hint.clear();
        Self::with_rows(
            colors,
            anchor,
            true,
            vec![Some(copy), Some(Self::entry(Command::EditPath, true, None))],
            keyboard,
            true,
            cx,
        )
    }
    pub fn custom(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        commands: Vec<Command>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_rows(
            colors,
            anchor,
            false,
            commands
                .into_iter()
                .map(|c| Some(Self::entry(c, false, None)))
                .collect(),
            keyboard,
            false,
            cx,
        )
    }
    pub fn origin(&self) -> Point<Pixels> {
        self.anchor
    }
    pub fn column_geometry(&self, level: usize) -> Option<ColumnGeometry> {
        let column = self.columns.get(level).filter(|c| c.ready)?;
        let width = if self.breadcrumb { 188. } else { 260. };
        let row_offset = if level == 0 {
            0.
        } else {
            let parent = &self.columns[level - 1];
            let at = parent.rows.iter().position(|r| {
                r.as_ref()
                    .is_some_and(|e| e.command == Command::Extension(column.parent, true))
            })?;
            parent
                .rows
                .iter()
                .take(at)
                .map(|r| {
                    if r.is_some() {
                        LAYOUT.floating.row_height
                    } else {
                        LAYOUT.floating.group_break_height
                    }
                })
                .sum::<f32>()
                + f32::from(parent.scroll.offset().y)
        };
        Some(ColumnGeometry {
            token: column.token,
            width: width + 16.,
            height: (Self::height(&column.rows) + 16.).min(640.),
            row_offset,
        })
    }
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
    fn height(rows: &[Option<Entry>]) -> f32 {
        rows.iter()
            .map(|r| {
                if r.is_some() {
                    LAYOUT.floating.row_height
                } else {
                    LAYOUT.floating.group_break_height
                }
            })
            .sum::<f32>()
            + 2. * LAYOUT.floating.padding_y
    }
    pub fn attach_shell(
        &mut self,
        result: Result<std::sync::Arc<crate::shell_menu::Session>, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(s) => {
                self.session = Some(s);
                self.status(0, "读取扩展…".into());
            }
            Err(e) => self.status(0, e),
        }
        cx.notify();
    }
    fn status(&mut self, level: usize, label: String) {
        if level == 0 {
            self.application_entry(Some(label));
            return;
        }
        let entry = Entry {
            command: Command::Extension(0, false),
            hint: String::new(),
            disabled: Some(label.clone()),
            label,
            checked: false,
            radio: false,
        };
        if let Some(c) = self.columns.get_mut(level) {
            c.rows = vec![Some(entry)];
            c.ready = true;
        }
    }
    fn application_entry(&mut self, disabled: Option<String>) {
        self.columns[0].rows.truncate(self.base);
        self.columns[0].rows.extend([
            None,
            Some(Entry {
                command: Command::Extension(0, true),
                label: crate::i18n::tr_owned("应用操作"),
                hint: if disabled.as_deref() == Some("读取扩展…") {
                    "…".into()
                } else {
                    String::new()
                },
                disabled,
                checked: false,
                radio: false,
            }),
        ]);
    }
    pub fn shell_notice(&mut self, notice: crate::shell_menu::Notice, cx: &mut Context<Self>) {
        if !self
            .session
            .as_ref()
            .is_some_and(|s| s.id == notice.session)
            || self.dismissed
        {
            return;
        }
        let level = if notice.parent == 0 {
            Some(0)
        } else {
            self.columns.iter().position(|c| c.parent == notice.parent)
        };
        let Some(level) = level else {
            return;
        };
        match notice.result {
            Err(_) => self.status(level, crate::i18n::tr_owned("扩展暂不可用")),
            Ok(items) => {
                let mut rows = Vec::new();
                for item in items {
                    if item.separator {
                        if rows.last().is_some_and(Option::is_some) {
                            rows.push(None)
                        }
                        continue;
                    }
                    rows.push(Some(Entry {
                        command: Command::Extension(item.id, item.submenu),
                        label: item.label,
                        hint: item.hint,
                        disabled: (!item.enabled).then(|| {
                            if item.unavailable {
                                "此扩展未提供可绘制的菜单项".into()
                            } else {
                                "此操作暂不可用".into()
                            }
                        }),
                        checked: item.checked,
                        radio: item.radio,
                    }));
                }
                while rows.last().is_some_and(Option::is_none) {
                    rows.pop();
                }
                if level == 0 {
                    self.application_entry(
                        rows.is_empty()
                            .then(|| crate::i18n::tr_owned("没有可用的应用操作")),
                    );
                    self.applications = Some(rows.clone());
                    if let Some(column) = self.columns.iter_mut().skip(1).find(|c| c.parent == 0) {
                        column.rows = rows;
                        column.ready = true;
                    }
                } else if rows.is_empty() {
                    // Keep the flyout at a normal row height with an explanatory
                    // disabled row instead of opening an empty bordered sliver.
                    self.status(level, crate::i18n::tr_owned("没有可用的菜单项"));
                } else {
                    self.columns[level].rows = rows;
                    self.columns[level].ready = true;
                }
            }
        }
        cx.notify();
    }
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.dismissed {
            self.dismissed = true;
            cx.emit(MenuEvent::Dismiss);
            cx.notify();
        }
    }
    fn open_submenu(&mut self, level: usize, id: u32, cx: &mut Context<Self>) {
        if self.columns.get(level + 1).is_some_and(|c| c.parent == id) {
            return;
        }
        if level >= 7 {
            return;
        }
        self.columns.truncate(level + 1);
        self.next_column = self.next_column.wrapping_add(1);
        self.columns.push(Column {
            scroll: ScrollHandle::new(),
            parent: id,
            rows: if id == 0 {
                self.applications.clone().unwrap_or_default()
            } else {
                vec![]
            },
            current: 0,
            token: self.next_column,
            ready: id == 0,
        });
        if id != 0 {
            self.status(level + 1, crate::i18n::tr_owned("读取扩展…"));
            self.columns[level + 1].ready = false;
        }
        if id != 0
            && let Some(session) = &self.session
            && let Err(e) = session.submenu(id)
        {
            self.status(level + 1, e)
        }
        cx.notify();
    }
    fn activate(&mut self, level: usize, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self
            .columns
            .get(level)
            .and_then(|c| c.rows.get(index))
            .and_then(Clone::clone)
        else {
            return;
        };
        if entry.disabled.is_some() {
            return;
        }
        match entry.command {
            Command::Extension(id, true) => {
                self.open_submenu(level, id, cx);
                self.active = (level + 1).min(self.columns.len() - 1);
                self.keyboard = true;
            }
            Command::Extension(id, false) => {
                if let Some(session) = &self.session {
                    match session.invoke(id) {
                        Ok(()) => self.dismiss(cx),
                        Err(e) => {
                            self.status(level, e);
                            cx.notify();
                        }
                    }
                }
            }
            command => {
                self.dismissed = true;
                cx.emit(MenuEvent::Command(command));
                cx.notify();
            }
        }
    }
    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if m.platform || (m.alt && key != "enter") {
            return;
        }
        let command = match (key, m.control, m.shift, m.alt) {
            ("l", true, false, false) if self.breadcrumb => Some(Command::EditPath),
            ("c", true, true, false) => Some(Command::CopyPath),
            ("c", true, false, false) => Some(Command::Copy),
            ("x", true, false, false) => Some(Command::Cut),
            ("v", true, false, false) => Some(Command::Paste),
            ("n", true, true, false) => Some(Command::NewFolder),
            ("f2", false, false, false) => Some(Command::Rename),
            ("f5", false, false, false) => Some(Command::Refresh),
            ("delete", false, false, false) => Some(Command::Recycle),
            ("enter", false, false, true) => Some(Command::Details),
            _ => None,
        };
        if let Some(command) = command {
            if self.columns[0]
                .rows
                .iter()
                .flatten()
                .any(|r| r.command == command && r.disabled.is_none())
            {
                self.dismissed = true;
                cx.emit(MenuEvent::Command(command));
            }
            cx.stop_propagation();
            return;
        }
        let level = self.active.min(self.columns.len() - 1);
        match key {
            "escape" | "tab" => {
                self.dismiss(cx);
                cx.stop_propagation();
            }
            "left" if level > 0 => {
                self.columns.truncate(level);
                self.active = level - 1;
                cx.notify();
                cx.stop_propagation();
            }
            "right" => {
                if matches!(
                    self.columns[level]
                        .rows
                        .get(self.columns[level].current)
                        .and_then(Option::as_ref)
                        .map(|e| e.command),
                    Some(Command::Extension(_, true))
                ) {
                    self.activate(level, self.columns[level].current, cx);
                }
                cx.stop_propagation();
            }
            "enter" | "space" => {
                self.activate(level, self.columns[level].current, cx);
                cx.stop_propagation();
            }
            "up" | "down" | "home" | "end" => {
                let enabled = self.columns[level]
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| r.as_ref().filter(|e| e.disabled.is_none()).map(|_| i))
                    .collect::<Vec<_>>();
                if !enabled.is_empty() {
                    let current = self.columns[level].current;
                    let at = enabled.iter().position(|i| *i == current).unwrap_or(0);
                    self.columns[level].current = match key {
                        "home" => enabled[0],
                        "end" => *enabled.last().unwrap(),
                        "up" => enabled[(at + enabled.len() - 1) % enabled.len()],
                        _ => enabled[(at + 1) % enabled.len()],
                    };
                    self.columns[level]
                        .scroll
                        .scroll_to_item(self.columns[level].current);
                    self.keyboard = true;
                    cx.notify();
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}
impl FileMenu {
    pub fn render_column(
        &mut self,
        level: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.columns.get(level).is_none_or(|c| !c.ready) {
            return div().size_full().into_any_element();
        }
        let colors = self.colors;
        let f = &LAYOUT.floating;
        let width = if self.breadcrumb { 188. } else { 260. };
        let active = self.active;
        let keyboard = self.keyboard;
        div()
            .id("menu-window-content")
            .font_family("Microsoft YaHei UI")
            .relative()
            .size_full()
            .when(level == 0, |d| {
                d.track_focus(&self.focus)
                    .tab_stop(true)
                    .on_key_down(cx.listener(Self::key))
            })
            .role(Role::Menu)
            .aria_label(if self.breadcrumb {
                "路径菜单"
            } else {
                "文件菜单"
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let width = if this.breadcrumb { 188. } else { 260. };
                    let inside = f32::from(event.position.x) >= 8.
                        && f32::from(event.position.x) <= 8. + width
                        && f32::from(event.position.y) >= 8.
                        && f32::from(event.position.y)
                            <= f32::from(window.viewport_size().height) - 8.;
                    if !inside {
                        this.dismiss(cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .children(
                std::iter::once((level, &self.columns[level])).map(|(level, column)| {
                    let x = 8.;
                    let y = 8.;
                    div()
                        .id(("menu-column", level))
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(width))
                        .flex()
                        .flex_col()
                        .track_scroll(&column.scroll)
                        .on_scroll(cx.listener(move |this, _: &ScrollEvent, _, cx| {
                            if this.columns.len() > level + 1 {
                                this.columns.truncate(level + 1);
                                this.active = level;
                                cx.notify();
                            }
                        }))
                        .h(px((f32::from(window.viewport_size().height) - 16.).max(32.)))
                        .px(px(f.padding_x))
                        .py(px(f.padding_y))
                        .rounded(px(f.radius))
                        .bg(colors.color(ColorRole::Floating))
                        .border_1()
                        .border_color(colors.color(ColorRole::Border))
                        .shadow(colors.control_shadows(false))
                        .occlude()
                        .overflow_y_scroll()
                        .children(column.rows.iter().enumerate().map(|(index, row)| {
                            let Some(entry) = row else {
                                return div()
                                    .h(px(f.group_break_height))
                                    .flex_none()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(1.))
                                            .bg(colors.color(ColorRole::Border)),
                                    )
                                    .into_any_element();
                            };
                            let disabled = entry.disabled.is_some();
                            let selected = column.current == index && level <= active;
                            let label = entry.label.clone();
                            let hint = entry.hint.clone();
                            let command = entry.command;
                            let is_sub = matches!(command, Command::Extension(_, true));
                            let checked = entry.checked;
                            let radio = entry.radio;
                            div()
                                .id(("menu-item", (level * 1024 + index)))
                                .role(Role::MenuItem)
                                .aria_label(format!(
                                    "{}{}{}",
                                    label,
                                    if checked { "，已勾选" } else { "" },
                                    if disabled { "，不可用" } else { "" }
                                ))
                                .w_full()
                                .h(px(f.row_height))
                                .flex_none()
                                .px(px(f.row_padding_x))
                                .rounded(px(f.row_radius))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .text_size(px(13.))
                                .text_color(colors.color(if disabled {
                                    ColorRole::Disabled
                                } else {
                                    ColorRole::Primary
                                }))
                                .when(selected && !disabled, |d| {
                                    d.bg(colors.color(if keyboard {
                                        ColorRole::InputActive
                                    } else {
                                        ColorRole::Hover
                                    }))
                                    .when(keyboard, |d| d.shadow(colors.control_shadows(true)))
                                })
                                .when_some(
                                    entry.disabled.clone().or_else(|| {
                                        (label.chars().count() > 24).then(|| label.clone())
                                    }),
                                    |d, message| d.tooltip(tooltip(message, colors)),
                                )
                                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                    if !*hovered || this.columns.get(level).is_none() {
                                        return;
                                    }
                                    this.hover_serial += 1;
                                    this.columns[level].current = index;
                                    this.active = level;
                                    this.keyboard = false;
                                    if let Command::Extension(id, true) = command {
                                        if !disabled {
                                            this.open_submenu(level, id, cx)
                                        }
                                    } else {
                                        let serial = this.hover_serial;
                                        cx.spawn(async move |menu, cx| {
                                            cx.background_executor()
                                                .timer(std::time::Duration::from_millis(150))
                                                .await;
                                            let _ = menu.update(cx, |this, cx| {
                                                if this.hover_serial == serial {
                                                    this.columns.truncate(level + 1);
                                                    cx.notify();
                                                }
                                            });
                                        })
                                        .detach();
                                    }
                                    cx.notify();
                                }))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this
                                        .columns
                                        .get(level)
                                        .and_then(|c| c.rows.get(index))
                                        .and_then(Option::as_ref)
                                        .is_some_and(|e| e.command == command)
                                    {
                                        this.activate(level, index, cx);
                                    }
                                    cx.stop_propagation();
                                }))
                                .child(div().w(px(12.)).flex_none().child(if checked {
                                    if radio { "●" } else { "✓" }
                                } else {
                                    ""
                                }))
                                .child(div().flex_1().min_w_0().truncate().child(label))
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(px(11.))
                                        .text_color(colors.color(ColorRole::Secondary))
                                        .child(if is_sub && hint.is_empty() {
                                            "›".to_string()
                                        } else {
                                            hint
                                        }),
                                )
                                .into_any_element()
                        }))
                        .into_any_element()
                }),
            )
            .into_any_element()
    }
}
impl Render for FileMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_column(0, window, cx)
    }
}
