//! One stable owned surface per ready menu column. Parent windows never resize.
use crate::menu::{ColumnGeometry, FileMenu, MenuEvent};
use gpui::{prelude::*, *};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    },
    System::Ole::RevokeDragDrop,
    UI::WindowsAndMessaging::*,
};
pub fn trace(message: &str) {
    static PATH: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    if let Some(path) = PATH.get_or_init(|| std::env::var_os("BACKLEAF_MENU_TRACE").map(Into::into))
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{message}");
        }
    }
}
#[derive(Clone, Copy)]
struct Placement {
    owner: HWND,
    owner_window: AnyWindowHandle,
    point: POINT,
    work: RECT,
    scale: f32,
}
impl Placement {
    fn for_parent(menu: &Entity<FileMenu>, parent: &Window, cx: &App) -> anyhow::Result<Self> {
        let owner = native(parent);
        let anchor = menu.read(cx).origin();
        let mut point = POINT {
            x: (f32::from(anchor.x) * parent.scale_factor()) as i32,
            y: (f32::from(anchor.y) * parent.scale_factor()) as i32,
        };
        #[cfg(test)]
        if owner.is_invalid() {
            return Ok(Self {
                owner,
                owner_window: parent.window_handle(),
                point,
                scale: parent.scale_factor(),
                work: RECT {
                    left: 0,
                    top: 0,
                    right: (f32::from(parent.viewport_size().width) * parent.scale_factor()) as i32,
                    bottom: (f32::from(parent.viewport_size().height) * parent.scale_factor())
                        as i32,
                },
            });
        }
        anyhow::ensure!(!owner.is_invalid(), "窗口句柄不可用");
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            ClientToScreen(owner, &mut point).ok()?;
            GetMonitorInfoW(
                MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .ok()?;
        }
        Ok(Self {
            owner,
            owner_window: parent.window_handle(),
            point,
            work: monitor.rcWork,
            scale: parent.scale_factor(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurfaceBounds {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    leftward: bool,
}
fn place(
    placement: Placement,
    column: ColumnGeometry,
    parent: Option<SurfaceBounds>,
) -> SurfaceBounds {
    let work = placement.work;
    let w =
        ((column.width * placement.scale).ceil() as i32).clamp(1, (work.right - work.left).max(1));
    let h =
        ((column.height * placement.scale).ceil() as i32).clamp(1, (work.bottom - work.top).max(1));
    let (x, y, leftward) = if let Some(parent) = parent {
        let overlap = (8. * placement.scale).round() as i32;
        let right = parent.x + parent.w - overlap;
        let left = parent.x - w + overlap;
        let leftward = (parent.leftward && left >= work.left) || right + w > work.right;
        (
            if leftward { left } else { right },
            parent.y + (column.row_offset * placement.scale).round() as i32,
            leftward,
        )
    } else {
        (placement.point.x, placement.point.y, false)
    };
    SurfaceBounds {
        x: x.clamp(work.left, work.right - w),
        y: y.clamp(work.top, work.bottom - h),
        w,
        h,
        leftward,
    }
}
#[derive(Clone, Copy)]
struct Member {
    token: u64,
    window: AnyWindowHandle,
    native: usize,
    bounds: SurfaceBounds,
}
#[derive(Default)]
struct Family {
    members: BTreeMap<usize, Member>,
    pending: BTreeSet<(usize, u64)>,
}
pub struct MenuPopup {
    // Reading Backleaf here would reroute the owner's entity/window subscriptions.
    menu: Entity<FileMenu>,
    level: usize,
    token: u64,
    placement: Placement,
    family: Rc<RefCell<Family>>,
    released: async_channel::Receiver<()>,
    _events: Subscription,
    _changed: Subscription,
    _activation: Subscription,
    _release: Subscription,
}
pub fn open(
    menu: Entity<FileMenu>,
    released: async_channel::Receiver<()>,
    parent: &mut Window,
    owner_view: WeakEntity<crate::Backleaf>,
    cx: &mut App,
) -> anyhow::Result<()> {
    let placement = Placement::for_parent(&menu, parent, cx)?;
    let geometry = menu
        .read(cx)
        .column_geometry(0)
        .ok_or_else(|| anyhow::anyhow!("菜单尚未就绪"))?;
    let bounds = place(placement, geometry, None);
    parent.defer(cx, move |_, cx| {
        let family = Rc::new(RefCell::new(Family::default()));
        if let Err(error) = open_column(
            menu,
            released,
            placement,
            family,
            (0, geometry.token),
            bounds,
            cx,
        ) {
            let _ = owner_view.update(cx, |root, cx| {
                root.files.menu = None;
                root.navigation.menu = None;
                root.navigation_error(error.to_string(), cx);
            });
        }
    });
    Ok(())
}
fn configure_native(
    popup: HWND,
    owner: HWND,
    level: usize,
    bounds: SurfaceBounds,
) -> windows::core::Result<()> {
    unsafe {
        SetWindowLongPtrW(popup, GWLP_HWNDPARENT, owner.0 as isize);
        let ex = (GetWindowLongPtrW(popup, GWL_EXSTYLE) as u32
            & !WS_EX_TOPMOST.0
            & !WS_EX_APPWINDOW.0
            & !WS_EX_NOACTIVATE.0)
            | WS_EX_TOOLWINDOW.0;
        SetWindowLongPtrW(
            popup,
            GWL_EXSTYLE,
            (if level > 0 {
                ex | WS_EX_NOACTIVATE.0
            } else {
                ex
            }) as isize,
        );
        SetWindowLongPtrW(
            popup,
            GWL_STYLE,
            ((GetWindowLongPtrW(popup, GWL_STYLE) as u32 & !WS_CHILD.0 & !WS_CAPTION.0)
                | WS_POPUP.0
                | WS_CLIPCHILDREN.0
                | WS_CLIPSIBLINGS.0) as isize,
        );
        let _ = RevokeDragDrop(popup);
        SetWindowPos(
            popup,
            Some(HWND_NOTOPMOST),
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            SWP_FRAMECHANGED | SWP_NOACTIVATE,
        )
    }
}
fn open_column(
    menu: Entity<FileMenu>,
    released: async_channel::Receiver<()>,
    placement: Placement,
    family: Rc<RefCell<Family>>,
    column: (usize, u64),
    bounds: SurfaceBounds,
    cx: &mut App,
) -> anyhow::Result<()> {
    let (level, token) = column;
    cx.open_window(
        WindowOptions {
            kind: WindowKind::PopUp,
            focus: level == 0,
            show: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Backleaf 文件菜单".into()),
                ..Default::default()
            }),
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(
                    px(bounds.x as f32 / placement.scale),
                    px(bounds.y as f32 / placement.scale),
                ),
                size(
                    px(bounds.w as f32 / placement.scale),
                    px(bounds.h as f32 / placement.scale),
                ),
            ))),
            is_resizable: false,
            is_minimizable: false,
            is_movable: false,
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx: &mut Context<MenuPopup>| {
                let popup = native(window);
                if !popup.is_invalid() {
                    if let Err(error) = configure_native(popup, placement.owner, level, bounds) {
                        trace(&format!("native menu placement failed: {error}"));
                    }
                    window.bounds_changed(cx);
                }
                let handle = window.window_handle();
                family.borrow_mut().members.insert(
                    level,
                    Member {
                        token,
                        window: handle,
                        native: popup.0 as usize,
                        bounds,
                    },
                );
                trace(&format!(
                    "column created level={level} token={token} hwnd={} bounds={bounds:?}",
                    popup.0 as usize
                ));
                if level == 0 {
                    if popup.is_invalid() {
                        window.activate_window();
                    }
                    window.focus(&menu.focus_handle(cx), cx);
                }
                let release_wait = released.clone();
                cx.spawn(async move |_, cx| {
                    let _ = release_wait.recv().await;
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                })
                .detach();
                let events = cx.subscribe_in(&menu, window, |_, _, _: &MenuEvent, window, _| {
                    window.remove_window()
                });
                let changed = cx.observe(&menu, |_, _, cx| cx.notify());
                let activation = cx.observe_window_activation(window, |_, window, cx| {
                    if window.is_window_active() {
                        return;
                    }
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(80))
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            let inside = if this.placement.owner.is_invalid() {
                                cx.active_window().is_some_and(|active| {
                                    this.family
                                        .borrow()
                                        .members
                                        .values()
                                        .any(|m| m.window == active)
                                })
                            } else {
                                let foreground = unsafe { GetForegroundWindow().0 as usize };
                                this.family
                                    .borrow()
                                    .members
                                    .values()
                                    .any(|m| m.native == foreground)
                            };
                            if !inside {
                                this.menu.update(cx, |m, cx| m.dismiss(cx));
                            }
                        });
                    })
                    .detach();
                });
                let release = cx.on_release(|this, cx| {
                    let mut family = this.family.borrow_mut();
                    if family
                        .members
                        .get(&this.level)
                        .is_some_and(|m| m.token == this.token)
                    {
                        family.members.remove(&this.level);
                    }
                    drop(family);
                    if this.level == 0 {
                        this.menu.update(cx, |m, cx| m.dismiss(cx));
                    }
                });
                MenuPopup {
                    menu,
                    level,
                    token,
                    placement,
                    family,
                    released,
                    _events: events,
                    _changed: changed,
                    _activation: activation,
                    _release: release,
                }
            })
        },
    )?;
    Ok(())
}

fn native(window: &Window) -> HWND {
    match raw_window_handle::HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Win32(raw)) => HWND(raw.hwnd.get() as *mut _),
        _ => HWND::default(),
    }
}
impl Render for MenuPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let valid = !self.released.is_closed()
            && cx.windows().contains(&self.placement.owner_window)
            && (self.placement.owner.is_invalid()
                || unsafe { IsWindow(Some(self.placement.owner)) }.as_bool())
            && !self.menu.read(cx).dismissed
            && self
                .menu
                .read(cx)
                .column_geometry(self.level)
                .is_some_and(|c| c.token == self.token);
        if !valid {
            window.remove_window();
            return div().into_any_element();
        }
        if self.level == 0 {
            let count = self.menu.read(cx).column_count();
            for level in 1..count {
                let Some(geometry) = self.menu.read(cx).column_geometry(level) else {
                    continue;
                };
                let mut family = self.family.borrow_mut();
                if family
                    .members
                    .get(&level)
                    .is_some_and(|m| m.token == geometry.token)
                    || !family.pending.insert((level, geometry.token))
                {
                    continue;
                }
                drop(family);
                let menu = self.menu.clone();
                let family = self.family.clone();
                let released = self.released.clone();
                let placement = self.placement;
                window.defer(cx, move |_, cx| {
                    family.borrow_mut().pending.remove(&(level, geometry.token));
                    if released.is_closed()
                        || menu.read(cx).dismissed
                        || menu
                            .read(cx)
                            .column_geometry(level)
                            .is_none_or(|c| c.token != geometry.token)
                    {
                        return;
                    }
                    let parent = family.borrow().members.get(&(level - 1)).copied();
                    let Some(parent) = parent else {
                        return;
                    };
                    let bounds = place(placement, geometry, Some(parent.bounds));
                    if let Err(error) = open_column(
                        menu.clone(),
                        released,
                        placement,
                        family,
                        (level, geometry.token),
                        bounds,
                        cx,
                    ) {
                        trace(&format!("column open failed level={level}: {error}"));
                        menu.update(cx, |m, cx| m.dismiss(cx));
                    }
                });
            }
        }
        self.menu.update(cx, |menu, menu_cx| {
            menu.render_column(self.level, window, menu_cx)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use windows::core::w;
    struct Hidden(HWND);
    impl Hidden {
        fn new() -> Self {
            Self(unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("Backleaf native geometry test"),
                    WS_POPUP,
                    0,
                    0,
                    300,
                    300,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            })
        }
    }
    impl Drop for Hidden {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    #[test]
    fn native_column_style_and_placement_never_mutate_the_parent() {
        let owner = Hidden::new();
        let root = Hidden::new();
        let child = Hidden::new();
        let root_bounds = SurfaceBounds {
            x: 60,
            y: 80,
            w: 276,
            h: 400,
            leftward: false,
        };
        configure_native(root.0, owner.0, 0, root_bounds).unwrap();
        let mut before = RECT::default();
        unsafe {
            GetWindowRect(root.0, &mut before).unwrap();
        }
        for height in [48, 200, 640] {
            configure_native(
                child.0,
                owner.0,
                1,
                SurfaceBounds {
                    x: 328,
                    y: 80,
                    w: 276,
                    h: height,
                    leftward: false,
                },
            )
            .unwrap();
            let mut after = RECT::default();
            unsafe {
                GetWindowRect(root.0, &mut after).unwrap();
            }
            assert_eq!(
                (after.left, after.top, after.right, after.bottom),
                (before.left, before.top, before.right, before.bottom)
            );
        }
        unsafe {
            assert_eq!(GetWindow(root.0, GW_OWNER).unwrap(), owner.0);
            assert_eq!(GetWindow(child.0, GW_OWNER).unwrap(), owner.0);
            assert_eq!(
                GetWindowLongPtrW(root.0, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE.0,
                0
            );
            assert_ne!(
                GetWindowLongPtrW(child.0, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE.0,
                0
            );
            assert!(!IsWindowVisible(root.0).as_bool() && !IsWindowVisible(child.0).as_bool());
        }
    }
}
