//! Background/idle policy for the index writer. Heavy directory crawling only runs
//! at full speed while the system is idle; while the user is active it trickles (and
//! the writer thread runs in background priority) so index maintenance stays
//! invisible. Notification batches are still applied every loop regardless of idle
//! state.
use std::time::Duration;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
};

/// System-wide input idle threshold for full-speed crawling.
pub const IDLE_THRESHOLD: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrawlBudget {
    pub max_directories: usize,
    pub window: Duration,
    pub work_sleep: Duration,
}

/// Full-speed while idle; a small trickle while the user is active so a large
/// initial build still eventually finishes and notifications keep flowing.
pub fn crawl_budget(idle: Duration) -> CrawlBudget {
    if idle >= IDLE_THRESHOLD {
        CrawlBudget {
            max_directories: 32,
            window: Duration::from_millis(500),
            work_sleep: Duration::from_millis(1),
        }
    } else {
        CrawlBudget {
            max_directories: 1,
            window: Duration::from_millis(20),
            work_sleep: Duration::from_millis(80),
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
        let active = crawl_budget(Duration::ZERO);
        let active_boundary = crawl_budget(IDLE_THRESHOLD - Duration::from_millis(1));
        let idle = crawl_budget(IDLE_THRESHOLD);
        assert_eq!(active, active_boundary);
        assert!(active.max_directories < idle.max_directories);
        assert!(active.window < idle.window);
        assert!(active.work_sleep > idle.work_sleep);
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
