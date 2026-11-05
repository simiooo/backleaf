//! Fast drive discovery is independent from two bounded, potentially slow media probes.
use crate::{model::Event, navigation::Notice};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}
impl Capacity {
    pub fn used(self) -> u64 {
        self.total.saturating_sub(self.free)
    }
    pub fn fraction(self) -> f32 {
        if self.total == 0 {
            0.
        } else {
            (self.used() as f64 / self.total as f64) as f32
        }
    }
    pub fn label(self) -> String {
        format!(
            "{} 剩余 / {}",
            bytes(self.free as u128),
            bytes(self.total as u128)
        )
    }
    pub fn description(self) -> String {
        format!(
            "总容量 {}；已使用 {}；剩余 {}；当前用户可用 {}",
            bytes(self.total as u128),
            bytes(self.used() as u128),
            bytes(self.free as u128),
            bytes(self.available as u128)
        )
    }
}
pub fn bytes(n: u128) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else if value >= 100. {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
#[derive(Default)]
struct Queue {
    pending: BTreeMap<PathBuf, u64>,
    inflight: BTreeSet<PathBuf>,
}
struct Shared {
    gate: Mutex<Queue>,
    changed: Condvar,
    stop: AtomicBool,
    active: AtomicBool,
    refresh: AtomicBool,
    epoch: AtomicU64,
}
pub struct Volumes(Arc<Shared>);
impl Volumes {
    pub fn new(tx: async_channel::Sender<Event>) -> Self {
        let shared = Arc::new(Shared {
            gate: Mutex::new(Queue::default()),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            active: AtomicBool::new(true),
            refresh: AtomicBool::new(true),
            epoch: AtomicU64::new(1),
        });
        for n in 0..2 {
            let state = shared.clone();
            let tx = tx.clone();
            thread::Builder::new()
                .name(format!("backleaf-volume-probe-{n}"))
                .spawn(move || {
                    error_mode();
                    loop {
                        let (path, epoch) = {
                            let mut q = state.gate.lock().unwrap();
                            loop {
                                if state.stop.load(Ordering::Acquire) {
                                    return;
                                }
                                let next = q
                                    .pending
                                    .iter()
                                    .find(|(p, _)| !q.inflight.contains(*p))
                                    .map(|(p, e)| (p.clone(), *e));
                                if let Some((path, epoch)) = next {
                                    q.pending.remove(&path);
                                    q.inflight.insert(path.clone());
                                    break (path, epoch);
                                }
                                q = state.changed.wait(q).unwrap();
                            }
                        };
                        let (title, value) = probe(&path);
                        {
                            let mut q = state.gate.lock().unwrap();
                            q.inflight.remove(&path);
                            state.changed.notify_all();
                        }
                        if state.stop.load(Ordering::Acquire) {
                            return;
                        }
                        if epoch == state.epoch.load(Ordering::Acquire)
                            && tx
                                .send_blocking(Event::Navigation(Notice::Capacity {
                                    epoch,
                                    path,
                                    title,
                                    value,
                                }))
                                .is_err()
                        {
                            return;
                        }
                    }
                })
                .expect("volume probe");
        }
        let state = shared.clone();
        thread::Builder::new()
            .name("backleaf-volumes".into())
            .spawn(move || {
                let _notification = Notification::new(&state);
                let mut previous = None;
                let mut published_epoch = 0;
                let mut sampled = Instant::now() - Duration::from_secs(30);
                let mut settle = Instant::now();
                loop {
                    let mut gate = state.gate.lock().unwrap();
                    while !state.active.load(Ordering::Acquire)
                        && !state.stop.load(Ordering::Acquire)
                    {
                        gate = state.changed.wait(gate).unwrap();
                    }
                    if state.stop.load(Ordering::Acquire) {
                        return;
                    }
                    let force = state.refresh.swap(false, Ordering::AcqRel);
                    drop(gate);
                    if force {
                        settle = Instant::now() + Duration::from_secs(2);
                    }
                    let next = crate::locations::read_volumes();
                    let changed = previous.as_ref() != Some(&next);
                    if changed {
                        state.epoch.fetch_add(1, Ordering::AcqRel);
                        previous = Some(next.clone());
                    }
                    let epoch = state.epoch.load(Ordering::Acquire);
                    if epoch != published_epoch {
                        published_epoch = epoch;
                        if tx
                            .send_blocking(Event::Navigation(Notice::VolumeSnapshot {
                                epoch,
                                result: next.clone(),
                            }))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if (force || changed || sampled.elapsed() >= Duration::from_secs(15))
                        && let Ok(rows) = next
                    {
                        let epoch = state.epoch.load(Ordering::Acquire);
                        let mut q = state.gate.lock().unwrap();
                        // At most one queued and one in-flight request per drive letter.
                        let live: BTreeSet<_> = rows.iter().map(|v| v.path.clone()).collect();
                        q.pending.retain(|p, _| live.contains(p));
                        for row in rows {
                            q.pending.insert(row.path, epoch);
                        }
                        sampled = Instant::now();
                        state.changed.notify_all();
                    }
                    let gate = state.gate.lock().unwrap();
                    if !state.refresh.load(Ordering::Acquire) && !state.stop.load(Ordering::Acquire)
                    {
                        let _ = state
                            .changed
                            .wait_timeout(
                                gate,
                                if Instant::now() < settle {
                                    Duration::from_millis(250)
                                } else {
                                    Duration::from_secs(2)
                                },
                            )
                            .unwrap();
                    }
                }
            })
            .expect("drive discovery");
        Self(shared)
    }
    pub fn epoch(&self) -> u64 {
        self.0.epoch.load(Ordering::Acquire)
    }
    pub fn set_active(&self, active: bool) {
        let _gate = self.0.gate.lock().unwrap();
        self.0.active.store(active, Ordering::Release);
        if active {
            self.0.refresh.store(true, Ordering::Release);
        }
        self.0.changed.notify_all();
    }
    pub fn refresh(&self) {
        let _gate = self.0.gate.lock().unwrap();
        self.0.refresh.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }
}
impl Drop for Volumes {
    fn drop(&mut self) {
        let _gate = self.0.gate.lock().unwrap();
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }
}
fn error_mode() -> bool {
    use windows::Win32::System::Diagnostics::Debug::*;
    unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX, None).is_ok() }
}
fn probe(path: &std::path::Path) -> (Option<String>, Option<Capacity>) {
    if !error_mode() {
        return (None, None);
    }
    use std::os::windows::ffi::OsStrExt;
    use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
    let raw: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let root = PCWSTR(raw.as_ptr());
    let kind = unsafe { GetDriveTypeW(root) };
    // Disconnected network paths cannot occupy both local media probe slots indefinitely.
    if !matches!(kind, 2 | 3 | 5 | 6) {
        return (None, None);
    }
    let mut label = [0u16; 261];
    let title = unsafe { GetVolumeInformationW(root, Some(&mut label), None, None, None, None) }
        .ok()
        .and_then(|_| {
            let name = String::from_utf16_lossy(
                &label[..label.iter().position(|c| *c == 0).unwrap_or(label.len())],
            );
            if name.is_empty() {
                None
            } else {
                Some(format!("{name} ({}:)", raw[0] as u8 as char))
            }
        });
    let mut info = DISK_SPACE_INFORMATION::default();
    let capacity = unsafe { GetDiskSpaceInformationW(root, &mut info) }
        .ok()
        .and_then(|_| {
            let unit = u64::from(info.SectorsPerAllocationUnit)
                .checked_mul(u64::from(info.BytesPerSector))?;
            let total = info.ActualTotalAllocationUnits.checked_mul(unit)?;
            let free = info.ActualAvailableAllocationUnits.checked_mul(unit)?;
            let available = info.CallerAvailableAllocationUnits.checked_mul(unit)?;
            (free <= total && available <= total).then_some(Capacity {
                total,
                free,
                available,
            })
        });
    (title, capacity)
}
struct Notification(windows::Win32::Devices::DeviceAndDriverInstallation::HCMNOTIFICATION);
impl Notification {
    fn new(shared: &Arc<Shared>) -> Option<Self> {
        use windows::Win32::Devices::DeviceAndDriverInstallation::*;
        let mut filter = CM_NOTIFY_FILTER {
            cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            ..Default::default()
        };
        filter.u.DeviceInterface = CM_NOTIFY_FILTER_0_0 {
            ClassGuid: windows_core::GUID::from_u128(0x53f5630d_b6bf_11d0_94f2_00a0c91efb8b),
        };
        let mut handle = HCMNOTIFICATION::default();
        (unsafe {
            CM_Register_Notification(
                &filter,
                Some(Arc::as_ptr(shared).cast()),
                Some(device_changed),
                &mut handle,
            )
        } == CR_SUCCESS)
            .then_some(Self(handle))
    }
}
impl Drop for Notification {
    fn drop(&mut self) {
        unsafe {
            let _ =
                windows::Win32::Devices::DeviceAndDriverInstallation::CM_Unregister_Notification(
                    self.0,
                );
        }
    }
}
unsafe extern "system" fn device_changed(
    _: windows::Win32::Devices::DeviceAndDriverInstallation::HCMNOTIFICATION,
    context: *const core::ffi::c_void,
    _: windows::Win32::Devices::DeviceAndDriverInstallation::CM_NOTIFY_ACTION,
    _: *const windows::Win32::Devices::DeviceAndDriverInstallation::CM_NOTIFY_EVENT_DATA,
    _: u32,
) -> u32 {
    // Registration is dropped and callbacks drained while the discovery thread still
    // owns this Arc. No filesystem, UI calls or unregister operation in the callback.
    let shared = unsafe { &*context.cast::<Shared>() };
    let _gate = shared.gate.lock().unwrap();
    shared.epoch.fetch_add(1, Ordering::AcqRel);
    shared.refresh.store(true, Ordering::Release);
    shared.changed.notify_all();
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_capacity_uses_physical_totals_and_quota_aware_available_bytes() {
        error_mode();
        let root = PathBuf::from(format!("{}\\", std::env::var("SystemDrive").unwrap()));
        let (_, capacity) = probe(&root);
        let capacity = capacity.expect("system volume capacity API");
        assert!(capacity.total > 0);
        assert!(capacity.free <= capacity.total);
        assert!(capacity.available <= capacity.total);
        assert!((0.0..=1.0).contains(&capacity.fraction()));
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        println!("system volume: {}", capacity.description());
    }
    #[test]
    fn native_pnp_registration_and_callback_invalidate_without_a_window() {
        use windows::Win32::Devices::DeviceAndDriverInstallation::*;
        let shared = Arc::new(Shared {
            gate: Mutex::new(Queue::default()),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            active: AtomicBool::new(true),
            refresh: AtomicBool::new(false),
            epoch: AtomicU64::new(1),
        });
        let registration = Notification::new(&shared).expect("volume interface PnP registration");
        unsafe {
            device_changed(
                HCMNOTIFICATION::default(),
                Arc::as_ptr(&shared).cast(),
                CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
                std::ptr::null(),
                0,
            );
        }
        assert_eq!(shared.epoch.load(Ordering::Acquire), 2);
        assert!(shared.refresh.load(Ordering::Acquire));
        drop(registration);
        assert_eq!(Arc::strong_count(&shared), 1);
    }
}
