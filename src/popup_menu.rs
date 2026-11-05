//! Owned native surface, independent from file content layout and clipping.
use crate::menu::{FileMenu, MenuEvent};
use gpui::{prelude::*, *};
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
struct Placement {
    owner: HWND,
    point: POINT,
    work: RECT,
}
impl Placement {
    fn for_parent(menu: &Entity<FileMenu>, parent: &Window, cx: &App) -> anyhow::Result<Self> {
        let owner = native(parent);
        let anchor = menu.read(cx).origin();
        let mut point = POINT {
            x: (f32::from(anchor.x) * parent.scale_factor()) as i32,
            y: (f32::from(anchor.y) * parent.scale_factor()) as i32,
        };
        // TestPlatform has independent windows but deliberately exposes no HWND.
        // Keep the actual MenuPopup/render/subscription path, replacing only OS geometry.
        #[cfg(test)]
        if owner.is_invalid() {
            return Ok(Self {
                owner,
                point,
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
            point,
            work: monitor.rcWork,
        })
    }
}
pub struct MenuPopup {
    // Do not read/observe the Backleaf entity from this window: GPUI associates
    // accessed entities with the rendering window, rerouting owner subscriptions.
    // Lifetime is signalled by OwnedMenu's lease instead.
    menu: Entity<FileMenu>,
    placement: Placement,
    owner_window: AnyWindowHandle,
    base: Option<POINT>,
    last: (i32, i32),
    released: async_channel::Receiver<()>,
    _events: Subscription,
    _changed: Subscription,
    _activation: Subscription,
}
pub fn open(
    menu: Entity<FileMenu>,
    released: async_channel::Receiver<()>,
    parent: &mut Window,
    owner_view: WeakEntity<crate::Backleaf>,
    cx: &mut App,
) -> anyhow::Result<()> {
    trace("open requested");
    let placement = Placement::for_parent(&menu, parent, cx)?;
    let owner = placement.owner;
    let owner_window = parent.window_handle();
    let report = owner_view.clone();
    parent.defer(cx, move |_, cx| {
        trace("open deferred");
        let result = cx.open_window(
            WindowOptions {
                kind: WindowKind::PopUp,
                titlebar: Some(TitlebarOptions {
                    title: Some("Backleaf 文件菜单".into()),
                    ..Default::default()
                }),
                window_background: WindowBackgroundAppearance::Transparent,
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    gpui::point(px(0.), px(0.)),
                    size(px(276.), px(240.)),
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
                        trace(&format!(
                            "created hwnd={} foreground={} visible={} style={:x}",
                            popup.0 as usize,
                            unsafe { GetForegroundWindow().0 as usize },
                            unsafe { IsWindowVisible(popup) }.as_bool(),
                            unsafe { GetWindowLongPtrW(popup, GWL_STYLE) }
                        ));
                    }
                    if !popup.is_invalid() {
                        unsafe {
                            SetWindowLongPtrW(popup, GWLP_HWNDPARENT, owner.0 as isize);
                            let ex = GetWindowLongPtrW(popup, GWL_EXSTYLE) as u32;
                            SetWindowLongPtrW(
                                popup,
                                GWL_EXSTYLE,
                                ((ex & !WS_EX_TOPMOST.0 & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0)
                                    as isize,
                            );
                            SetWindowLongPtrW(
                                popup,
                                GWL_STYLE,
                                ((GetWindowLongPtrW(popup, GWL_STYLE) as u32
                                    & !WS_CHILD.0
                                    & !WS_CAPTION.0)
                                    | WS_POPUP.0
                                    | WS_CLIPCHILDREN.0
                                    | WS_CLIPSIBLINGS.0) as isize,
                            );
                            let _ = RevokeDragDrop(popup);
                            let _ = SetWindowPos(
                                popup,
                                Some(HWND_NOTOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE,
                            );
                        }
                    }
                    if popup.is_invalid() {
                        // TestPlatform does not perform native CreateWindow activation.
                        window.activate_window();
                    }
                    window.focus(&menu.focus_handle(cx), cx);
                    let release_wait = released.clone();
                    let popup_handle = window.window_handle();
                    cx.spawn(async move |_, cx| {
                        let _ = release_wait.recv().await;
                        let _ = popup_handle.update(cx, |_, window, _| window.remove_window());
                    })
                    .detach();
                    let events =
                        cx.subscribe_in(&menu, window, |_, _, _: &MenuEvent, window, _| {
                            window.remove_window()
                        });
                    let changed = cx.observe(&menu, |_, _, cx| cx.notify());
                    let activation = cx.observe_window_activation(window, |this, window, cx| {
                        trace(&format!("activation {}", window.is_window_active()));
                        if !window.is_window_active() {
                            let popup = native(window).0 as usize;
                            if popup == 0 {
                                this.menu.update(cx, |menu, cx| menu.dismiss(cx));
                                return;
                            }
                            cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(80))
                                    .await;
                                let _ = this.update(cx, |this, cx| {
                                    if popup != 0
                                        && unsafe { GetForegroundWindow().0 as usize } != popup
                                    {
                                        trace("dismiss foreground changed");
                                        this.menu.update(cx, |m, cx| m.dismiss(cx));
                                    }
                                });
                            })
                            .detach();
                        }
                        cx.notify();
                    });
                    MenuPopup {
                        menu,
                        placement,
                        owner_window,
                        base: None,
                        last: (0, 0),
                        released,
                        _events: events,
                        _changed: changed,
                        _activation: activation,
                    }
                })
            },
        );
        if let Err(error) = result {
            let _ = report.update(cx, |root, cx| {
                root.files.menu = None;
                root.navigation.menu = None;
                root.navigation_error(error.to_string(), cx);
            });
        }
    });
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
        let attached = !self.released.is_closed();
        if !attached
            || !cx.windows().contains(&self.owner_window)
            || (!self.placement.owner.is_invalid()
                && !unsafe { IsWindow(Some(self.placement.owner)) }.as_bool())
            || self.menu.read(cx).dismissed
        {
            trace(&format!(
                "remove attached={attached} owner_valid={} dismissed={}",
                unsafe { IsWindow(Some(self.placement.owner)) }.as_bool(),
                self.menu.read(cx).dismissed
            ));
            window.remove_window();
        }
        let scale = window.scale_factor();
        let (width, height) = self.menu.read(cx).dimensions();
        let work = &self.placement.work;
        let w = ((width * scale).ceil() as i32).min(work.right - work.left);
        let h = ((height.min(640.) * scale).ceil() as i32).min(work.bottom - work.top);
        if self.base.is_none() {
            let p = self.placement.point;
            let left = p.x + ((536. * scale) as i32) > work.right
                && p.x - work.left > ((268. * scale) as i32);
            self.menu.update(cx, |m, _| m.set_leftward(left));
            self.base = Some(POINT {
                x: p.x.clamp(work.left, work.right - w),
                y: p.y.clamp(work.top, work.bottom - h),
            });
        }
        if self.last != (w, h) {
            let base = self.base.unwrap();
            let x = if self.menu.read(cx).leftward() {
                base.x + w.min((276. * scale) as i32) - w
            } else {
                base.x
            };
            let x = x.clamp(work.left, work.right - w);
            let y = base.y.clamp(work.top, work.bottom - h);
            let menu = self.menu.clone();
            window.defer(cx, move |window, cx| {
                if native(window).is_invalid() {
                    window.resize(size(
                        px(w as f32 / window.scale_factor()),
                        px(h as f32 / window.scale_factor()),
                    ));
                } else {
                    unsafe {
                        let _ = SetWindowPos(
                            native(window),
                            None,
                            x,
                            y,
                            w,
                            h,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                // Native resize callbacks can re-enter while this window is leased.
                // Reconcile explicitly after SetWindowPos so layout uses its new client size.
                window.bounds_changed(cx);
                window.refresh();
                menu.update(cx, |_, cx| cx.notify());
            });
            self.last = (w, h);
        }
        div().size_full().child(self.menu.clone())
    }
}
