//! OS session notifications; no global hooks, polling animation timers or desktop input.
use crate::model::Event;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener},
            LibraryLoader::GetModuleHandleW,
            RemoteDesktop::*,
            Threading::GetCurrentThreadId,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{PWSTR, w},
};

thread_local! { static EVENTS:RefCell<Option<async_channel::Sender<Event>>>=const{RefCell::new(None)}; }
pub struct SessionMonitor {
    stop: Arc<AtomicBool>,
    thread: Arc<AtomicU32>,
}
impl Drop for SessionMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let id = self.thread.load(Ordering::Acquire);
        if id != 0 {
            unsafe {
                let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
}
pub fn locked() -> anyhow::Result<bool> {
    unsafe {
        let mut buffer = PWSTR::null();
        let mut bytes = 0;
        WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )?;
        let result = if bytes as usize >= std::mem::size_of::<WTSINFOEXW>() && !buffer.is_null() {
            let info = &*(buffer.0 as *const WTSINFOEXW);
            if info.Level == 1 {
                let state = info.Data.WTSInfoExLevel1;
                Ok(state.SessionState != WTSActive
                    || state.SessionFlags as u32 != WTS_SESSIONSTATE_UNLOCK)
            } else {
                Err(anyhow::anyhow!("unknown WTS info level"))
            }
        } else {
            Err(anyhow::anyhow!("incomplete WTS session info"))
        };
        WTSFreeMemory(buffer.0.cast());
        result
    }
}
fn send(locked: bool) {
    EVENTS.with_borrow(|tx| {
        if let Some(tx) = tx {
            let _ = tx.send_blocking(Event::Session { locked });
        }
    });
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLIPBOARDUPDATE {
        EVENTS.with_borrow(|tx| {
            if let Some(tx) = tx {
                let _ = tx.try_send(Event::ClipboardChanged);
            }
        });
    }
    if message == WM_WTSSESSION_CHANGE {
        match wparam.0 as u32 {
            WTS_SESSION_LOCK | WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => send(true),
            WTS_SESSION_UNLOCK => send(false),
            WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT | 0xF => send(locked().unwrap_or(true)), // WTS_SESSION_DESKTOP_READY
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
pub fn monitor(tx: async_channel::Sender<Event>) -> SessionMonitor {
    let stop = Arc::new(AtomicBool::new(false));
    let thread = Arc::new(AtomicU32::new(0));
    let (stop_flag, thread_id) = (stop.clone(), thread.clone());
    std::thread::Builder::new()
        .name("backleaf-session".into())
        .spawn(move || {
            EVENTS.with_borrow_mut(|events| *events = Some(tx));
            let result = (|| -> anyhow::Result<()> {
                unsafe {
                    let module = GetModuleHandleW(None)?;
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(procedure),
                        hInstance: module.into(),
                        lpszClassName: w!("BackleafSessionNotifications"),
                        ..Default::default()
                    };
                    RegisterClassW(&class);
                    let hwnd = CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        class.lpszClassName,
                        w!(""),
                        WINDOW_STYLE::default(),
                        0,
                        0,
                        0,
                        0,
                        Some(HWND_MESSAGE),
                        None,
                        Some(module.into()),
                        None,
                    )?;
                    if let Err(error) =
                        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)
                    {
                        let _ = DestroyWindow(hwnd);
                        return Err(error.into());
                    }
                    thread_id.store(GetCurrentThreadId(), Ordering::Release);
                    let _ = AddClipboardFormatListener(hwnd);
                    send(locked().unwrap_or(true));
                    let mut message = MSG::default();
                    while !stop_flag.load(Ordering::Acquire)
                        && GetMessageW(&mut message, None, 0, 0).0 > 0
                    {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    let _ = WTSUnRegisterSessionNotification(hwnd);
                    let _ = RemoveClipboardFormatListener(hwnd);
                    let _ = DestroyWindow(hwnd);
                    Ok(())
                }
            })();
            if let Err(error) = result {
                eprintln!("session monitor: {error}");
                send(true);
            }
            thread_id.store(0, Ordering::Release);
            EVENTS.with_borrow_mut(|events| *events = None);
        })
        .expect("session monitor thread");
    SessionMonitor { stop, thread }
}
