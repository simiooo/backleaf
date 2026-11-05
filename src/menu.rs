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
}
impl Command {
    fn label(self, folder: bool) -> &'static str {
        match self {
            Self::Open => "打开",
            Self::Copy => "复制",
            Self::Cut => "剪切",
            Self::Paste => {
                if folder {
                    "粘贴到文件夹"
                } else {
                    "粘贴"
                }
            }
            Self::CopyPath => "复制路径",
            Self::Rename => "重命名",
            Self::Recycle => "移至回收站",
            Self::Details => "详情",
            Self::NewFolder => "新建文件夹",
            Self::Refresh => "刷新",
            Self::EditPath => "编辑路径",
            Self::Compress => "压缩为 ZIP…",
            Self::Extract => "解压到…",
            Self::Extension(_, _) => "",
            Self::PinQuickAccess => "固定到侧栏",
            Self::RemoveQuickAccess => "移除入口",
            Self::RenameQuickView => "重命名视图",
            Self::MoveQuickUp => "上移",
            Self::MoveQuickDown => "下移",
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
            Self::PinQuickAccess | Self::RemoveQuickAccess | Self::RenameQuickView => "",
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
    y: f32,
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
    leftward: bool,
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
            vec![Some(NewFolder), Some(Paste), None, Some(Refresh)]
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
                v.push(Some(PinQuickAccess))
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
                            Some("渲染基准不执行文件命令".into())
                        } else if command == Rename && selected > 1 {
                            Some("一次重命名一个项目".into())
                        } else if command == Paste && !paste {
                            Some("剪贴板中没有文件".into())
                        } else {
                            None
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        Self::with_rows(colors, anchor, folder, rows, keyboard, false, cx)
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
                parent: 0,
                rows,
                current,
                y: 0.,
            }],
            keyboard,
            breadcrumb,
            active: 0,
            base,
            session: None,
            dismissed: false,
            hover_serial: 0,
            leftward: false,
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
    pub fn set_leftward(&mut self, left: bool) {
        self.leftward = left
    }
    pub fn leftward(&self) -> bool {
        self.leftward
    }
    pub fn dimensions(&self) -> (f32, f32) {
        let width = if self.breadcrumb { 188. } else { 260. };
        let visible = self.columns.len().min(3);
        let height = self
            .columns
            .iter()
            .rev()
            .take(visible)
            .map(|c| c.y + Self::height(&c.rows))
            .fold(0., f32::max);
        (
            width * visible as f32 + 8. * visible.saturating_sub(1) as f32 + 16.,
            height + 16.,
        )
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
        let entry = Entry {
            command: Command::Extension(0, false),
            hint: String::new(),
            disabled: Some(label.clone()),
            label,
            checked: false,
            radio: false,
        };
        if level == 0 {
            self.columns[0].rows.truncate(self.base);
            self.columns[0].rows.extend([None, Some(entry)]);
        } else if let Some(c) = self.columns.get_mut(level) {
            c.rows = vec![Some(entry)];
        }
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
        let Some(level) = self.columns.iter().position(|c| c.parent == notice.parent) else {
            return;
        };
        match notice.result {
            Err(_) => self.status(level, "扩展暂不可用".into()),
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
                    self.columns[0].rows.truncate(self.base);
                    if !rows.is_empty() {
                        self.columns[0].rows.push(None);
                        self.columns[0].rows.extend(rows)
                    }
                } else {
                    self.columns[level].rows = rows;
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
        let y = self.columns[level].y
            + LAYOUT.floating.padding_y
            + self.columns[level]
                .rows
                .iter()
                .take(self.columns[level].current)
                .map(|r| {
                    if r.is_some() {
                        LAYOUT.floating.row_height
                    } else {
                        LAYOUT.floating.group_break_height
                    }
                })
                .sum::<f32>();
        self.columns.truncate(level + 1);
        self.columns.push(Column {
            scroll: ScrollHandle::new(),
            parent: id,
            rows: vec![],
            current: 0,
            y,
        });
        self.status(level + 1, "读取扩展…".into());
        if let Some(session) = &self.session
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
                self.active = level + 1;
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
impl Render for FileMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let f = &LAYOUT.floating;
        let width = if self.breadcrumb { 188. } else { 260. };
        let fit = (((f32::from(window.viewport_size().width) - 8.) / (width + 8.)).floor()
            as usize)
            .clamp(1, 3);
        let start = self.columns.len().saturating_sub(fit);
        let count = self.columns.len() - start;
        let active = self.active;
        let keyboard = self.keyboard;
        let left = self.leftward;
        div()
            .id("menu-window-content")
            .font_family("Microsoft YaHei UI")
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .tab_stop(true)
            .role(Role::Menu)
            .aria_label(if self.breadcrumb {
                "路径菜单"
            } else {
                "文件菜单"
            })
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    let width = if this.breadcrumb { 188. } else { 260. };
                    let count = this.columns.len().min(3);
                    let inside = (0..count).any(|i| {
                        let col = &this.columns[this.columns.len() - count + i];
                        let at = if this.leftward { count - 1 - i } else { i };
                        let x = 8. + at as f32 * (width + 8.);
                        let y = 8. + col.y;
                        f32::from(event.position.x) >= x
                            && f32::from(event.position.x) <= x + width
                            && f32::from(event.position.y) >= y
                            && f32::from(event.position.y) <= y + Self::height(&col.rows)
                    });
                    if !inside {
                        this.dismiss(cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .children(
                self.columns
                    .iter()
                    .enumerate()
                    .skip(start)
                    .map(|(level, column)| {
                        let x = 8.
                            + if left {
                                (count - 1 - (level - start)) as f32
                            } else {
                                (level - start) as f32
                            } * (width + 8.);
                        let y = (8. + column.y).min(
                            (f32::from(window.viewport_size().height)
                                - Self::height(&column.rows)
                                - 8.)
                                .max(8.),
                        );
                        div()
                            .id(("menu-column", level))
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .w(px(width))
                            .flex()
                            .flex_col()
                            .track_scroll(&column.scroll)
                            .max_h(px(
                                (f32::from(window.viewport_size().height) - y - 8.).max(48.)
                            ))
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
                                        if !*hovered {
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
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.activate(level, index, cx);
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
                                            .child(if is_sub { "›".to_string() } else { hint }),
                                    )
                                    .into_any_element()
                            }))
                            .into_any_element()
                    }),
            )
    }
}
