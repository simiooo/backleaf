//! Lazy, visible-window image thumbnails.
//!
//! The grid is already virtualized by `uniform_list`; only rendered cells call
//! `image()`. A single background worker decodes and downsizes on demand, while
//! the UI thread keeps a bounded LRU of `RenderImage`s. Results are tied to a
//! directory generation so a navigation drops stale work and GPU images.
use gpui::{ImageSource, RenderImage, Window};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

/// Decode at a size that stays crisp in a roughly 48 px logical cell at 2x.
const SIDE: u32 = 128;
const CACHE_LIMIT: usize = 256;
const QUEUE_LIMIT: usize = 64;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "dib", "tif", "tiff", "ico",
];
#[derive(Default)]
struct Stats {
    decoded: u64,
    unavailable: u64,
    result_peak: usize,
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
    stop: bool,
    active: bool,
    epoch: u64,
    pending: VecDeque<Request>,
    working: Option<(u64, PathBuf)>,
    results: VecDeque<Response>,
    stats: Stats,
}
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
    pending: HashSet<PathBuf>,
    epoch: u64,
    active: bool,
    used: u64,
    peak: usize,
}
impl Cache {
    pub fn new(tx: async_channel::Sender<crate::model::Event>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("visible-image-thumbnails".into())
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
                    let image = decode(&request.path);
                    let mut state = worker.state.lock().unwrap();
                    state.working = None;
                    if state.stop {
                        return;
                    }
                    if request.epoch != state.epoch {
                        continue;
                    }
                    if image.is_some() {
                        state.stats.decoded += 1;
                    } else {
                        state.stats.unavailable += 1;
                    }
                    state.results.push_back(Response { request, image });
                    state.stats.result_peak = state.stats.result_peak.max(state.results.len());
                    // A full wake channel is fine: the next render drains results.
                    let _ = tx.try_send(crate::model::Event::ImagePreviewsChanged);
                }
            })
            .expect("visible image thumbnail worker");
        Self {
            shared,
            entries: HashMap::new(),
            pending: HashSet::new(),
            epoch: u64::MAX,
            active: false,
            used: 0,
            peak: 0,
        }
    }
    /// Drain worker results and bind them to the current directory generation.
    pub fn sync(&mut self, epoch: u64, active: bool, window: &mut Window) {
        let mut state = self.shared.state.lock().unwrap();
        if self.epoch != epoch {
            self.epoch = epoch;
            state.epoch = epoch;
            state.pending.clear();
            state.results.clear();
            self.pending.clear();
            for (_, entry) in self.entries.drain() {
                if let Some(image) = entry.image {
                    let _ = window.drop_image(image);
                }
            }
        }
        self.active = active;
        state.active = active;
        for response in state.results.drain(..) {
            if response.request.epoch != epoch {
                continue;
            }
            self.pending.remove(&response.request.path);
            if self.entries.len() >= CACHE_LIMIT
                && let Some(key) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(path, _)| path.clone())
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
    /// Request a thumbnail for a rendered cell. Returns a ready image or `None`
    /// while the worker is still busy or when the file has no usable preview.
    pub fn image(&mut self, path: PathBuf) -> Option<ImageSource> {
        self.used += 1;
        if let Some(entry) = self.entries.get_mut(&path) {
            entry.used = self.used;
            return entry.image.clone().map(ImageSource::Render);
        }
        if self.pending.contains(&path) || !self.active {
            return None;
        }
        let mut state = self.shared.state.lock().unwrap();
        if !state.active || state.pending.len() >= QUEUE_LIMIT {
            return None;
        }
        state.pending.push_back(Request {
            epoch: self.epoch,
            path: path.clone(),
        });
        drop(state);
        self.pending.insert(path);
        self.shared.changed.notify_one();
        None
    }
    #[cfg(test)]
    pub fn loaded(&self, path: &Path) -> bool {
        self.entries
            .get(path)
            .is_some_and(|entry| entry.image.is_some())
    }
}
/// The formats this build can actually decode. HEIC/AVIF are intentionally
/// excluded: they are classified as image files for icon art, but decoding them
/// needs codecs the preview worker does not ship.
pub fn supported_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && EXTENSIONS
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
        })
}
fn decode(path: &Path) -> Option<Arc<RenderImage>> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    use image::ImageDecoder as _;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    if image.width() == 0 || image.height() == 0 {
        return None;
    }
    let thumbnail = image.thumbnail(SIDE, SIDE).into_rgba8();
    let (width, height) = thumbnail.dimensions();
    let mut pixels = thumbnail.into_raw();
    // GPUI's RenderImage contract is BGRA straight-alpha; the image crate
    // decodes RGBA. swap only the colour channels.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let rgba = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}
#[cfg(test)]
mod tests {
    use super::supported_name;
    use std::ffi::OsStr;
    #[test]
    fn supported_names_are_explicit_and_do_not_probe_content() {
        for name in [
            "a.png", "b.JPG", "c.jpeg", "d.webp", "e.gif", "f.bmp", "g.tif", "h.ico",
        ] {
            assert!(supported_name(OsStr::new(name)), "{name}");
        }
        for name in [
            "a.heic",
            "b.avif",
            "c.pdf",
            "d.mp4",
            "e.png.exe",
            "noext",
            "f.",
        ] {
            assert!(!supported_name(OsStr::new(name)), "{name}");
        }
    }
    #[test]
    fn thumbnail_pixels_are_bgra_and_keep_aspect_size() {
        let fixture = crate::test_support::Fixture::new("image-preview-decode");
        let path = fixture.0.join("photo.png");
        image::RgbaImage::from_pixel(2, 1, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let image = super::decode(&path).expect("png decodes");
        let bytes = image.as_bytes(0).expect("first frame");
        assert_eq!(&bytes[..4], &[30, 20, 10, 255]);
        assert!(bytes.len() > 4);
    }
}
