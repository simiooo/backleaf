use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
};

/// System-wide input idle threshold for full-speed crawling.
pub const IDLE_THRESHOLD: Duration = Duration::from_secs(15);

/// Whether the app currently has a visible, active window. Crawling keeps a low
/// duty cycle while the user is looking at the app, and runs at full speed only
/// when the window is in the background/hidden and the system is idle.
static WINDOW_FOREGROUND: AtomicBool = AtomicBool::new(false);

pub fn set_window_foreground(foreground: bool) {
    WINDOW_FOREGROUND.store(foreground, Ordering::Relaxed);
}

fn window_foreground() -> bool {
    WINDOW_FOREGROUND.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrawlBudget {
    pub max_directories: usize,
    pub window: Duration,
    pub work_sleep: Duration,
    pub directory_sleep: Duration,
}

/// Full-speed while the system is idle and the window is in the background; a
/// low foreground-idle duty cycle so a long build cannot fan the CPU while the
/// user is reading the app; a small trickle while input is active.
pub fn crawl_budget(idle: Duration) -> CrawlBudget {
    crawl_budget_for(idle, window_foreground())
}

fn crawl_budget_for(idle: Duration, foreground: bool) -> CrawlBudget {
    if idle >= IDLE_THRESHOLD {
        if foreground {
            CrawlBudget {
                max_directories: 8,
                window: Duration::from_millis(500),
                work_sleep: Duration::from_millis(1),
                directory_sleep: Duration::from_millis(12),
            }
        } else {
            CrawlBudget {
                max_directories: 32,
                window: Duration::from_millis(500),
                work_sleep: Duration::from_millis(1),
                directory_sleep: Duration::ZERO,
            }
        }
    } else {
        CrawlBudget {
            max_directories: 1,
            window: Duration::from_millis(20),
            work_sleep: Duration::from_millis(80),
            directory_sleep: Duration::ZERO,
        }
    }
}

/// Runs the calling thread in Windows background mode: lowest scheduling priority
/// and background I/O priority, so an index build does not compete with foreground
/// work.
pub fn enter_background_priority() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount() -> u32;
}

#[cfg(test)]
static FORCE_IDLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Tests/benchmarks that drive the real `Service` must not depend on whether the
/// developer touched the machine recently.
#[cfg(test)]
pub fn force_idle() {
    FORCE_IDLE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Time since the last system-wide input event. A failure to query the input
/// desktop is treated as "active" so maintenance stays conservative.
pub fn system_idle() -> Duration {
    #[cfg(test)]
    {
        if FORCE_IDLE.load(std::sync::atomic::Ordering::Relaxed) {
            return IDLE_THRESHOLD;
        }
    }
    let mut info = LastInputInfo {
        cb_size: std::mem::size_of::<LastInputInfo>() as u32,
        dw_time: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return Duration::ZERO;
    }
    let now = unsafe { GetTickCount() };
    Duration::from_millis(now.wrapping_sub(info.dw_time) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_system_gets_a_smaller_crawl_budget_than_idle() {
        let active = crawl_budget_for(Duration::ZERO, false);
        let active_boundary = crawl_budget_for(IDLE_THRESHOLD - Duration::from_millis(1), false);
        let idle = crawl_budget_for(IDLE_THRESHOLD, false);
        assert_eq!(active, active_boundary);
        assert!(active.max_directories < idle.max_directories);
        assert!(active.window < idle.window);
        assert!(active.work_sleep > idle.work_sleep);
        assert_eq!(active.directory_sleep, Duration::ZERO);
    }

    #[test]
    fn foreground_idle_crawls_more_slowly_than_background_idle() {
        let background = crawl_budget_for(IDLE_THRESHOLD, false);
        let foreground = crawl_budget_for(IDLE_THRESHOLD, true);
        assert!(foreground.max_directories < background.max_directories);
        assert!(foreground.directory_sleep > background.directory_sleep);
        assert_eq!(background.directory_sleep, Duration::ZERO);
        assert_eq!(foreground.window, background.window);
    }

    #[test]
    fn system_idle_never_panics_and_respects_the_test_override() {
        force_idle();
        assert_eq!(system_idle(), IDLE_THRESHOLD);
        FORCE_IDLE.store(false, std::sync::atomic::Ordering::Relaxed);
        // The real query either returns a duration or conservatively zero.
        let _ = system_idle();
    }
}
