//! Bounded, read-only extraction of an application's embedded icon. No Shell
//! association handlers, execution, file scans, or disk work on the UI thread.
use gpui::{ImageSource, RenderImage, Window};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

const SIDE: usize = 96;
const QUEUE_LIMIT: usize = 32;
const CACHE_LIMIT: usize = 128;

#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct Stats {
    pub cache_entries: usize,
    pub cache_peak: usize,
    pub queue_peak: usize,
    pub result_peak: usize,
    pub extracted: usize,
    pub unavailable: usize,
}
struct Request {
    epoch: u64,
    path: PathBuf,
}
struct Response {
    request: Request,
    image: Option<Arc<RenderImage>>,
}
#[derive(Default)]
struct State {
    epoch: u64,
    active: bool,
    stop: bool,
    pending: VecDeque<Request>,
    working: Option<(u64, PathBuf)>,
    results: VecDeque<Response>,
    stats: Stats,
}
impl State {
    fn enqueue(&mut self, request: Request) -> bool {
        if self.stop
            || !self.active
            || request.epoch != self.epoch
            || self.pending.len() >= QUEUE_LIMIT
            || self
                .pending
                .iter()
                .any(|r| r.epoch == request.epoch && r.path == request.path)
            || self
                .results
                .iter()
                .any(|r| r.request.epoch == request.epoch && r.request.path == request.path)
            || self
                .working
                .as_ref()
                .is_some_and(|(epoch, path)| *epoch == request.epoch && *path == request.path)
        {
            return false;
        }
        self.pending.push_back(request);
        self.stats.queue_peak = self.stats.queue_peak.max(self.pending.len());
        true
    }
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
struct Entry {
    image: Option<Arc<RenderImage>>,
    used: u64,
}
pub struct Cache {
    shared: Arc<Shared>,
    entries: HashMap<PathBuf, Entry>,
    epoch: u64,
    used: u64,
    peak: usize,
}
impl Cache {
    pub fn new(tx: async_channel::Sender<crate::model::Event>) -> Self {
        let shared = Arc::new(Shared::default());
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("embedded-application-icons".into())
            .spawn(move || {
                loop {
                    let request = {
                        let mut state = worker.state.lock().unwrap();
                        while !state.stop
                            && (!state.active
                                || state.pending.is_empty()
                                || state.results.len() >= QUEUE_LIMIT)
                        {
                            state = worker.changed.wait(state).unwrap();
                        }
                        if state.stop {
                            return;
                        }
                        let request = state.pending.pop_front().unwrap();
                        state.working = Some((request.epoch, request.path.clone()));
                        request
                    };
                    let image = extract(&request.path).map(|pixels| {
                        Arc::new(RenderImage::new(vec![image::Frame::new(
                            image::RgbaImage::from_raw(SIDE as u32, SIDE as u32, pixels).unwrap(),
                        )]))
                    });
                    let mut state = worker.state.lock().unwrap();
                    state.working = None;
                    if state.stop {
                        return;
                    }
                    if request.epoch == state.epoch {
                        if image.is_some() {
                            state.stats.extracted += 1;
                        } else {
                            state.stats.unavailable += 1;
                        }
                        state.results.push_back(Response { request, image });
                        state.stats.result_peak = state.stats.result_peak.max(state.results.len());
                        // The shared result queue owns the result if the wake channel is full.
                        // Any already queued event/render will drain it; no result is lost.
                        let _ = tx.try_send(crate::model::Event::ApplicationIconsChanged);
                    }
                }
            })
            .expect("application icon worker");
        Self {
            shared,
            entries: HashMap::new(),
            epoch: 0,
            used: 0,
            peak: 0,
        }
    }
    pub fn sync(&mut self, epoch: u64, active: bool, window: &mut Window) {
        let mut state = self.shared.state.lock().unwrap();
        state.active = active;
        if self.epoch != epoch {
            self.epoch = epoch;
            state.epoch = epoch;
            state.pending.clear();
            state.results.clear();
            for (_, entry) in self.entries.drain() {
                if let Some(image) = entry.image {
                    let _ = window.drop_image(image);
                }
            }
        }
        for response in state.results.drain(..) {
            if response.request.epoch != epoch {
                continue;
            }
            if self.entries.len() >= CACHE_LIMIT
                && let Some(key) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, e)| e.used)
                    .map(|(p, _)| p.clone())
                && let Some(entry) = self.entries.remove(&key)
                && let Some(image) = entry.image
            {
                let _ = window.drop_image(image);
            }
            self.used += 1;
            self.entries.insert(
                response.request.path,
                Entry {
                    image: response.image,
                    used: self.used,
                },
            );
            self.peak = self.peak.max(self.entries.len());
        }
        self.shared.changed.notify_one();
    }
    pub fn image(&mut self, path: PathBuf) -> Option<ImageSource> {
        self.used += 1;
        if let Some(entry) = self.entries.get_mut(&path) {
            entry.used = self.used;
            return entry.image.clone().map(ImageSource::Render);
        }
        if self.shared.state.lock().unwrap().enqueue(Request {
            epoch: self.epoch,
            path,
        }) {
            self.shared.changed.notify_one();
        }
        None
    }
    pub fn stats(&self) -> Stats {
        let mut stats = self.shared.state.lock().unwrap().stats;
        stats.cache_entries = self.entries.len();
        stats.cache_peak = self.peak;
        stats
    }
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().stop = true;
        self.shared.changed.notify_all();
        // Never join a possibly slow filesystem/resource read on the UI thread.
    }
}

/// BGRA straight-alpha pixels, matching GPUI's RenderImage contract.
fn extract(path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::{
        Graphics::Gdi::*,
        UI::{Shell::SHDefExtractIconW, WindowsAndMessaging::*},
    };
    use windows_core::PCWSTR;
    struct Icon(HICON);
    impl Drop for Icon {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyIcon(self.0);
            }
        }
    }
    struct Canvas {
        dc: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
        bits: *mut u8,
    }
    impl Drop for Canvas {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.previous);
                let _ = DeleteObject(self.bitmap.into());
                let _ = DeleteDC(self.dc);
            }
        }
    }
    let before = std::fs::metadata(path).ok()?;
    if !before.is_file() {
        return None;
    }
    let mut wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|c| if c == 47 { 92 } else { c })
        .collect();
    if wide.contains(&0) {
        return None;
    }
    wide.push(0);
    let mut handle = HICON::default();
    unsafe {
        let result = SHDefExtractIconW(
            PCWSTR(wide.as_ptr()),
            0,
            0,
            Some(&mut handle),
            None,
            SIDE as u32,
        );
        if handle.is_invalid() {
            return None;
        }
        let icon = Icon(handle);
        if result.is_err() {
            return None;
        }
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIDE as i32,
                biHeight: -(SIDE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) if !bits.is_null() => bitmap,
            Ok(bitmap) => {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(dc);
                return None;
            }
            _ => {
                let _ = DeleteDC(dc);
                return None;
            }
        };
        let previous = SelectObject(dc, bitmap.into());
        let canvas = Canvas {
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
        };
        let pixels = std::slice::from_raw_parts_mut(canvas.bits, SIDE * SIDE * 4);
        pixels.fill(0);
        for p in pixels.as_chunks_mut::<4>().0.iter_mut() {
            p[3] = 255;
        }
        DrawIconEx(
            dc,
            0,
            0,
            icon.0,
            SIDE as i32,
            SIDE as i32,
            0,
            None,
            DI_NORMAL,
        )
        .ok()?;
        let _ = GdiFlush();
        let mut black = pixels.to_vec();
        pixels.fill(255);
        DrawIconEx(
            dc,
            0,
            0,
            icon.0,
            SIDE as i32,
            SIDE as i32,
            0,
            None,
            DI_NORMAL,
        )
        .ok()?;
        let _ = GdiFlush();
        // Two backgrounds reconstruct alpha for both modern alpha icons and
        // legacy AND-mask icons, avoiding black boxes or dark edge fringes.
        for (b, w) in black
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels.as_chunks::<4>().0.iter())
        {
            let alpha = 255 - (0..3).map(|i| w[i].saturating_sub(b[i])).max().unwrap();
            for channel in &mut b[..3] {
                *channel = if alpha == 0 {
                    0
                } else {
                    ((*channel as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
                };
            }
            b[3] = alpha;
        }
        let after = std::fs::metadata(path).ok()?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            return None;
        }
        fit_visible_artwork(black)
    }
}

fn fit_visible_artwork(mut pixels: Vec<u8>) -> Option<Vec<u8>> {
    let maximum = pixels.as_chunks::<4>().0.iter().map(|p| p[3]).max()?;
    if maximum == 0 {
        return None;
    }
    let threshold = maximum.min(128);
    let (mut left, mut top, mut right, mut bottom) = (SIDE, SIDE, 0, 0);
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[3] >= threshold {
            left = left.min(i % SIDE);
            right = right.max(i % SIDE + 1);
            top = top.min(i / SIDE);
            bottom = bottom.max(i / SIDE + 1);
        }
    }
    // Filter premultiplied pixels so transparent edges do not acquire dark halos.
    for p in pixels.as_chunks_mut::<4>().0.iter_mut() {
        for i in 0..3 {
            p[i] = (p[i] as u16 * p[3] as u16 / 255) as u8;
        }
    }
    let source = image::RgbaImage::from_raw(SIDE as u32, SIDE as u32, pixels)?;
    let crop = image::imageops::crop_imm(
        &source,
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    );
    let limit = SIDE * 40 / 44;
    let scale = limit as f32 / (right - left).max(bottom - top) as f32;
    let width = (((right - left) as f32 * scale).round() as usize).max(1);
    let height = (((bottom - top) as f32 * scale).round() as usize).max(1);
    let scaled = image::imageops::resize(
        &crop.to_image(),
        width as u32,
        height as u32,
        image::imageops::FilterType::Triangle,
    );
    let mut result = vec![0; SIDE * SIDE * 4];
    for (i, p) in scaled.as_raw().as_chunks::<4>().0.iter().enumerate() {
        let target =
            (((SIDE - height) / 2 + i / width) * SIDE + (SIDE - width) / 2 + i % width) * 4;
        let alpha = p[3];
        for channel in 0..3 {
            result[target + channel] = if alpha == 0 {
                0
            } else {
                ((p[channel] as u32 * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
            };
        }
        result[target + 3] = alpha;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{QUEUE_LIMIT, Request, SIDE, State, extract, fit_visible_artwork};
    #[test]
    fn transparent_padding_does_not_shrink_the_visible_icon() {
        let mut input = vec![0; SIDE * SIDE * 4];
        for y in 36..60 {
            for x in 24..72 {
                let pixel = (y * SIDE + x) * 4;
                input[pixel..pixel + 4].copy_from_slice(&[30, 120, 60, 255]);
            }
        }
        let output = fit_visible_artwork(input).unwrap();
        let indices: Vec<_> = output
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[3] > 128)
            .map(|(i, _)| i)
            .collect();
        let width = indices.iter().map(|i| i % SIDE).max().unwrap()
            - indices.iter().map(|i| i % SIDE).min().unwrap()
            + 1;
        let height = indices.iter().map(|i| i / SIDE).max().unwrap()
            - indices.iter().map(|i| i / SIDE).min().unwrap()
            + 1;
        assert_eq!(width, SIDE * 40 / 44);
        assert!((height as i32 * 2 - width as i32).abs() <= 1);
        assert_eq!(output[3], 0);
        assert!(fit_visible_artwork(vec![0; SIDE * SIDE * 4]).is_none());
    }
    #[test]
    fn requests_are_bounded_deduplicated_and_generation_scoped() {
        let mut state = State {
            epoch: 7,
            active: true,
            ..Default::default()
        };
        for i in 0..1000 {
            state.enqueue(Request {
                epoch: 7,
                path: format!("fixture-{i}.exe").into(),
            });
        }
        assert_eq!(state.pending.len(), QUEUE_LIMIT);
        assert_eq!(state.stats.queue_peak, QUEUE_LIMIT);
        assert!(!state.enqueue(Request {
            epoch: 6,
            path: "stale.exe".into()
        }));
        state.pending.clear();
        state.working = Some((7, "working.exe".into()));
        assert!(!state.enqueue(Request {
            epoch: 7,
            path: "working.exe".into()
        }));
        assert!(state.enqueue(Request {
            epoch: 7,
            path: "ready.exe".into()
        }));
        assert!(!state.enqueue(Request {
            epoch: 7,
            path: "ready.exe".into()
        }));
        state.active = false;
        assert!(!state.enqueue(Request {
            epoch: 7,
            path: "hidden.exe".into()
        }));
    }
    #[test]
    fn real_embedded_resource_raster_has_color_and_transparency() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/windows/backleaf.ico");
        for _ in 0..8 {
            let pixels = extract(&path).expect("native project icon resource");
            assert_eq!(pixels.len(), SIDE * SIDE * 4);
            assert_eq!(pixels[3], 0);
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 240 && p[1] > p[2])
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255)
            );
        }
        assert!(extract(&path.with_file_name("not-created-backleaf-test.exe")).is_none());
    }
}
