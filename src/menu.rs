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
    ShellActions,
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
            Self::ShellActions => "其他应用操作…",
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
            Self::Compress | Self::Extract | Self::ShellActions => "",
            Self::PinQuickAccess | Self::RemoveQuickAccess | Self::RenameQuickView => "",
            Self::MoveQuickUp => "Alt+↑",
            Self::MoveQuickDown => "Alt+↓",
        }
    }
}
pub enum MenuEvent {
    Command(Command),
    Dismiss,
}
struct Entry {
    command: Command,
    disabled: Option<&'static str>,
}
pub struct FileMenu {
    colors: &'static Palette,
    focus: FocusHandle,
    anchor: Point<Pixels>,
    folder: bool,
    rows: Vec<Option<Entry>>,
    current: usize,
    keyboard: bool,
    breadcrumb: bool,
}
impl EventEmitter<MenuEvent> for FileMenu {}
impl Focusable for FileMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl FileMenu {
    #[expect(
        clippy::too_many_arguments,
        reason = "Context menu combines its frozen file context with placement, palette and input modality"
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
        let commands = if selected == 0 {
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
            let mut commands = vec![Some(Open), None, Some(Copy), Some(Cut)];
            if folder {
                commands.push(Some(Paste));
            } else {
                commands.push(Some(CopyPath));
            }
            commands.extend([None, Some(Rename), Some(Recycle), None, Some(Details)]);
            if folder {
                commands.push(Some(PinQuickAccess));
            }
            commands
        };
        let mut commands = commands;
        if selected > 0 {
            commands.extend([None, Some(Compress)]);
            if !folder {
                commands.push(Some(Extract));
            }
        }
        commands.extend([None, Some(ShellActions)]);
        let rows = commands
            .into_iter()
            .map(|command| {
                command.map(|command| Entry {
                    command,
                    disabled: if !real && !matches!(command, Details) {
                        Some("渲染基准不执行文件命令")
                    } else if command == Rename && selected > 1 {
                        Some("一次重命名一个项目")
                    } else if command == Paste && !paste {
                        Some("剪贴板中没有可粘贴的文件")
                    } else {
                        None
                    },
                })
            })
            .collect::<Vec<_>>();
        let current = rows
            .iter()
            .position(|r| r.as_ref().is_some_and(|r| r.disabled.is_none()))
            .unwrap_or(0);
        Self {
            colors,
            focus: cx.focus_handle().tab_stop(true),
            anchor,
            folder,
            rows,
            current,
            keyboard,
            breadcrumb: false,
        }
    }
    pub fn breadcrumb(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            colors,
            focus: cx.focus_handle().tab_stop(true),
            anchor,
            folder: true,
            rows: vec![
                Some(Entry {
                    command: Command::CopyPath,
                    disabled: None,
                }),
                Some(Entry {
                    command: Command::EditPath,
                    disabled: None,
                }),
            ],
            current: 0,
            keyboard,
            breadcrumb: true,
        }
    }
    pub fn custom(
        colors: &'static Palette,
        anchor: Point<Pixels>,
        commands: Vec<Command>,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            colors,
            focus: cx.focus_handle().tab_stop(true),
            anchor,
            folder: false,
            rows: commands
                .into_iter()
                .map(|command| {
                    Some(Entry {
                        command,
                        disabled: None,
                    })
                })
                .collect(),
            current: 0,
            keyboard,
            breadcrumb: false,
        }
    }
    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(Some(entry)) = self.rows.get(index)
            && entry.disabled.is_none()
        {
            cx.emit(MenuEvent::Command(entry.command));
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
            ("d", true, false, false) => Some(Command::Recycle),
            ("n", true, true, false) => Some(Command::NewFolder),
            ("f2", false, false, false) => Some(Command::Rename),
            ("f5", false, false, false) => Some(Command::Refresh),
            ("delete", false, false, false) => Some(Command::Recycle),
            ("enter", false, false, true) => Some(Command::Details),
            _ => None,
        };
        if let Some(command) = command {
            if let Some(index) = self
                .rows
                .iter()
                .position(|r| r.as_ref().is_some_and(|r| r.command == command))
            {
                self.activate(index, cx);
                cx.stop_propagation();
            }
            return;
        }
        if m.alt || m.control {
            return;
        }
        match key {
            "escape" | "tab" | "menu" => {
                cx.emit(MenuEvent::Dismiss);
                cx.stop_propagation();
            }
            "f10" if m.shift => {
                cx.emit(MenuEvent::Dismiss);
                cx.stop_propagation();
            }
            "enter" | "space" => {
                self.activate(self.current, cx);
                cx.stop_propagation();
            }
            "up" | "down" | "home" | "end" => {
                let enabled = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| r.as_ref().filter(|r| r.disabled.is_none()).map(|_| i))
                    .collect::<Vec<_>>();
                if !enabled.is_empty() {
                    let at = enabled.iter().position(|i| *i == self.current).unwrap_or(0);
                    self.current = match key {
                        "home" => enabled[0],
                        "end" => *enabled.last().unwrap(),
                        "up" => enabled[(at + enabled.len() - 1) % enabled.len()],
                        _ => enabled[(at + 1) % enabled.len()],
                    };
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
        let height = self
            .rows
            .iter()
            .map(|r| {
                if r.is_some() {
                    f.row_height
                } else {
                    f.group_break_height
                }
            })
            .sum::<f32>()
            + f.padding_y * 2.;
        let width = if self.breadcrumb { 188. } else { 260. };
        let x = f32::from(self.anchor.x).clamp(
            8.,
            (f32::from(window.viewport_size().width) - width - 8.).max(8.),
        );
        let y = f32::from(self.anchor.y).clamp(
            8.,
            (f32::from(window.viewport_size().height) - height - 8.).max(8.),
        );
        div()
            .id("file-context-menu")
            .role(Role::Menu)
            .aria_label(if self.breadcrumb {
                "路径菜单"
            } else {
                "文件菜单"
            })
            .track_focus(&self.focus)
            .tab_stop(true)
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .px(px(f.padding_x))
            .py(px(f.padding_y))
            .flex()
            .flex_col()
            .bg(colors.color(ColorRole::Floating))
            .border_1()
            .border_color(colors.color(ColorRole::Border))
            .rounded(px(f.radius))
            .shadow(vec![colors.shadow()])
            .occlude()
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down_out(cx.listener(|_, event: &MouseDownEvent, _, cx| {
                cx.emit(MenuEvent::Dismiss);
                if event.button == MouseButton::Left {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .max_h(px((f32::from(window.viewport_size().height) - 24.).max(48.)))
            .overflow_y_scroll()
            .children(self.rows.iter().enumerate().map(|(i, row)| {
                if let Some(entry) = row {
                    let disabled = entry.disabled.is_some();
                    let selected = self.current == i;
                    let label = if self.breadcrumb && entry.command == Command::CopyPath {
                        "复制文件路径"
                    } else {
                        entry.command.label(self.folder)
                    };
                    div()
                        .id(("menu-row", i))
                        .role(Role::MenuItem)
                        .aria_label(label)
                        .w_full()
                        .h(px(f.row_height))
                        .px(px(f.row_padding_x))
                        .rounded(px(f.row_radius))
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .text_size(px(13.))
                        .text_color(colors.color(if disabled {
                            ColorRole::Disabled
                        } else {
                            ColorRole::Primary
                        }))
                        .when(selected && !disabled, |d| {
                            d.bg(colors.color(ColorRole::Hover))
                        })
                        .when(selected && self.keyboard && !disabled, |d| {
                            d.shadow(vec![colors.focus_ring()])
                        })
                        .when_some(entry.disabled, |d, reason| {
                            d.tooltip(tooltip(reason.into(), colors))
                        })
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered {
                                this.current = i;
                                this.keyboard = false;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate(i, cx);
                            cx.stop_propagation();
                        }))
                        .child(div().flex_1().child(label))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(colors.color(if disabled {
                                    ColorRole::Disabled
                                } else {
                                    ColorRole::Secondary
                                }))
                                .child(if self.breadcrumb {
                                    ""
                                } else {
                                    entry.command.shortcut()
                                }),
                        )
                        .into_any_element()
                } else {
                    div()
                        .w_full()
                        .h(px(f.group_break_height))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .child(div().w_full().h(px(1.)).bg(colors.color(ColorRole::Border)))
                        .into_any_element()
                }
            }))
    }
}
