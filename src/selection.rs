//! A sparse inverse for Select All, compressed arbitrary selections, and one latest-only worker.
use roaring::RoaringTreemap;
use std::{
    ops::Range,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

#[derive(Clone, Default)]
pub struct SelectionMask {
    inverse: Option<usize>,
    values: Arc<RoaringTreemap>,
}
impl SelectionMask {
    pub fn same_snapshot(&self, other: &Self) -> bool {
        self.inverse == other.inverse && Arc::ptr_eq(&self.values, &other.values)
    }
    pub fn from_bitmap(mut values: RoaringTreemap) -> Self {
        values.optimize();
        Self {
            inverse: None,
            values: Arc::new(values),
        }
    }
    pub fn contains(&self, id: usize) -> bool {
        match self.inverse {
            Some(end) => id < end && !self.values.contains(id as u64),
            None => self.values.contains(id as u64),
        }
    }
    pub fn count(&self) -> usize {
        self.inverse.map_or(self.values.len() as usize, |end| {
            end - self.values.len() as usize
        })
    }
    pub fn bitmap_serialized_bytes(&self) -> usize {
        self.values.serialized_size()
    }
    /// Stream a frozen selection on a worker without allocating all selected IDs.
    pub fn ids_from(&self, start: usize, end: usize) -> Box<dyn Iterator<Item = usize> + '_> {
        if let Some(prefix) = self.inverse {
            Box::new((start..end.min(prefix)).filter(|id| !self.values.contains(*id as u64)))
        } else {
            let mut values = self.values.iter();
            values.advance_to(start as u64);
            Box::new(
                values
                    .take_while(move |id| *id < end as u64)
                    .map(|id| id as usize),
            )
        }
    }
    pub fn count_before(&self, end: usize) -> usize {
        match self.inverse {
            Some(prefix) => {
                let limit = prefix.min(end);
                limit - self.values.range_cardinality(0..limit as u64) as usize
            }
            None => self.values.range_cardinality(0..end as u64) as usize,
        }
    }
    pub fn exclude(&mut self, removed: &RoaringTreemap) {
        let values = Arc::make_mut(&mut self.values);
        if let Some(end) = self.inverse {
            *values |= removed;
            values.remove_range(end as u64..=u64::MAX);
        } else {
            *values -= removed;
        }
    }
    fn range(range: Range<usize>) -> Self {
        let mut values = RoaringTreemap::new();
        values.insert_range(range.start as u64..range.end as u64);
        Self {
            inverse: None,
            values: Arc::new(values),
        }
    }
    fn materialize(&self) -> RoaringTreemap {
        if let Some(end) = self.inverse {
            let mut values = RoaringTreemap::new();
            values.insert_range(0..end as u64);
            values -= self.values.as_ref();
            values
        } else {
            self.values.as_ref().clone()
        }
    }
    fn toggle(&mut self, id: usize) {
        if self.inverse.is_some_and(|end| id >= end) {
            self.values = Arc::new(self.materialize());
            self.inverse = None;
        }
        let values = Arc::make_mut(&mut self.values);
        if !values.remove(id as u64) {
            values.insert(id as u64);
        }
    }
    fn extend_range(&mut self, range: Range<usize>) {
        if self.inverse.is_some_and(|end| range.end <= end) {
            Arc::make_mut(&mut self.values).remove_range(range.start as u64..range.end as u64);
        } else {
            if self.inverse.is_some() {
                self.values = Arc::new(self.materialize());
                self.inverse = None;
            }
            Arc::make_mut(&mut self.values).insert_range(range.start as u64..range.end as u64);
        }
    }
    fn with_band(&self, band: GridBand, additive: bool, cancel: impl Fn() -> bool) -> Option<Self> {
        if cancel() {
            return None;
        }
        if band.is_empty() {
            return Some(if additive {
                self.clone()
            } else {
                Self::default()
            });
        }
        if !additive && band.len() == band.count {
            return Some(Self {
                inverse: Some(band.count),
                values: Arc::default(),
            });
        }
        if additive && self.inverse.is_some_and(|end| band.end_id() <= end) {
            // Union with an almost-full snapshot only removes sparse exceptions.
            let kept = self.values.iter().filter(|id| !band.contains(*id as usize));
            let values = RoaringTreemap::from_sorted_iter(kept).expect("sorted exceptions");
            return Some(Self {
                inverse: self.inverse,
                values: Arc::new(values),
            });
        }
        let mut values = if additive {
            self.materialize()
        } else {
            RoaringTreemap::new()
        };
        if band.first_col == 0 && band.last_col == band.columns {
            values.insert_range((band.first_row * band.columns) as u64..band.end_id() as u64);
        } else {
            for (index, range) in band.ranges().enumerate() {
                if index % 128 == 0 && cancel() {
                    return None;
                }
                values.insert_range(range.start as u64..range.end as u64);
            }
        }
        if cancel() {
            return None;
        }
        // Repeated disjoint ranges may retain interval containers larger than a bitmap.
        // Normalize on the worker before publishing the immutable selection snapshot.
        values.optimize();
        if cancel() {
            return None;
        }
        Some(Self {
            inverse: None,
            values: Arc::new(values),
        })
    }
}

#[derive(Clone, Default)]
pub struct Selection {
    pub mask: SelectionMask,
    pub focus: Option<usize>,
    pub anchor: usize,
}
impl Selection {
    pub fn contains(&self, id: usize) -> bool {
        self.mask.contains(id)
    }
    pub fn count(&self) -> usize {
        self.mask.count()
    }
    pub fn select(&mut self, id: usize, control: bool, shift: bool) {
        if shift {
            let range = self.anchor.min(id)..self.anchor.max(id) + 1;
            if control {
                self.mask.extend_range(range);
            } else {
                self.mask = SelectionMask::range(range);
            }
        } else if control {
            self.mask.toggle(id);
            self.anchor = id;
        } else {
            self.mask = SelectionMask::range(id..id + 1);
            self.anchor = id;
        }
        self.focus = Some(id);
    }
    pub fn all(&mut self, count: usize) {
        self.mask = SelectionMask {
            inverse: Some(count),
            values: Arc::default(),
        };
        if self.focus.is_none() && count > 0 {
            self.focus = Some(0);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridBand {
    pub first_row: usize,
    pub last_row: usize,
    pub first_col: usize,
    pub last_col: usize,
    pub columns: usize,
    pub count: usize,
}
impl GridBand {
    pub fn is_empty(&self) -> bool {
        self.first_row >= self.last_row || self.first_col >= self.last_col || self.count == 0
    }
    pub fn contains(&self, id: usize) -> bool {
        !self.is_empty()
            && id < self.count
            && (self.first_row..self.last_row).contains(&(id / self.columns))
            && (self.first_col..self.last_col).contains(&(id % self.columns))
    }
    pub fn end_id(&self) -> usize {
        ((self.last_row.saturating_sub(1)) * self.columns + self.last_col).min(self.count)
    }
    pub fn first_id(&self) -> Option<usize> {
        self.ranges().next().map(|r| r.start)
    }
    pub fn last_id(&self) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        let mut row = self.last_row - 1;
        if row * self.columns + self.first_col >= self.count {
            row = row.checked_sub(1)?;
        }
        (row >= self.first_row).then(|| (row * self.columns + self.last_col).min(self.count) - 1)
    }
    pub fn len(&self) -> usize {
        if self.is_empty() {
            return 0;
        }
        let full = self.last_row - self.first_row - 1;
        full * (self.last_col - self.first_col)
            + self
                .end_id()
                .saturating_sub((self.last_row - 1) * self.columns + self.first_col)
    }
    fn ranges(self) -> impl Iterator<Item = Range<usize>> {
        (self.first_row..self.last_row)
            .map(move |row| {
                row * self.columns + self.first_col
                    ..(row * self.columns + self.last_col).min(self.count)
            })
            .filter(|r| r.start < r.end)
    }
    /// Coordinates are global f64 row positions, never GPUI's enormous f32 scroll height.
    pub fn from_rect(
        columns: usize,
        cell_width: f64,
        count: usize,
        a: (f64, f64),
        b: (f64, f64),
    ) -> Self {
        let columns = columns.max(1);
        let rows = count.div_ceil(columns);
        let (left, right) = (a.0.min(b.0), a.0.max(b.0));
        let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
        if left == right || top == bottom {
            return Self {
                columns,
                count,
                ..Self::default()
            };
        }
        let column_step = cell_width + crate::layout::LAYOUT.grid.column_gap as f64;
        let row_step = crate::layout::row_step() as f64;
        let first_col =
            (((left - cell_width) / column_step).floor() + 1.).clamp(0., columns as f64) as usize;
        let last_col = (right / column_step).ceil().clamp(0., columns as f64) as usize;
        let first_row = (((top - crate::layout::LAYOUT.grid.cell_height as f64) / row_step).floor()
            + 1.)
            .clamp(0., rows as f64) as usize;
        let last_row = (bottom / row_step).ceil().clamp(0., rows as f64) as usize;
        Self {
            first_row,
            last_row,
            first_col,
            last_col,
            columns,
            count,
        }
    }
}

pub struct SelectionResult {
    pub token: u64,
    pub mask: SelectionMask,
}
struct Request {
    token: u64,
    base: SelectionMask,
    band: GridBand,
    additive: bool,
}
pub struct SelectionWorker {
    pending: Arc<Mutex<Option<Request>>>,
    latest: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    worker: thread::Thread,
}
impl SelectionWorker {
    pub fn new(tx: async_channel::Sender<SelectionResult>) -> Self {
        let pending = Arc::new(Mutex::new(None::<Request>));
        let latest = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (jobs, serial, stopping) = (pending.clone(), latest.clone(), stop.clone());
        let handle = thread::Builder::new()
            .name("backleaf-selection".into())
            .spawn(move || {
                loop {
                    if stopping.load(Ordering::Relaxed) || tx.is_closed() {
                        break;
                    }
                    let job = { jobs.lock().unwrap().take() };
                    let Some(request) = job else {
                        thread::park();
                        continue;
                    };
                    let cancel = || {
                        stopping.load(Ordering::Relaxed)
                            || serial.load(Ordering::Relaxed) != request.token
                    };
                    if let Some(mask) =
                        request
                            .base
                            .with_band(request.band, request.additive, cancel)
                        && !cancel()
                        && tx
                            .send_blocking(SelectionResult {
                                token: request.token,
                                mask,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("selection worker");
        Self {
            pending,
            latest,
            stop,
            worker: handle.thread().clone(),
        }
    }
    pub fn request(&self, base: SelectionMask, band: GridBand, additive: bool) -> u64 {
        let token = self.latest.fetch_add(1, Ordering::Relaxed) + 1;
        *self.pending.lock().unwrap() = Some(Request {
            token,
            base,
            band,
            additive,
        });
        self.worker.unpark();
        token
    }
    pub fn cancel(&self) {
        self.latest.fetch_add(1, Ordering::Relaxed);
        *self.pending.lock().unwrap() = None;
    }
}
impl Drop for SelectionWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.cancel();
        self.worker.unpark();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_and_exceptions_do_not_scale_with_ten_million_items() {
        let mut s = Selection::default();
        s.all(10_000_000);
        s.select(9_999_999, true, false);
        assert_eq!(s.count(), 9_999_999);
        assert!(!s.contains(9_999_999));
        assert!(s.mask.bitmap_serialized_bytes() < 8192);
        s.select(9_999_999, true, false);
        assert_eq!(s.count(), 10_000_000);
    }
    #[test]
    fn control_shift_extends_without_losing_disjoint_selection() {
        let mut s = Selection::default();
        s.select(900, false, false);
        s.select(5, true, false);
        s.select(9, true, true);
        assert_eq!(s.count(), 6);
        assert!(s.contains(900));
        assert!((6..9).all(|id| s.contains(id)));
        s.all(10_000_000);
        s.select(900, true, false);
        s.select(905, true, true);
        assert_eq!(s.count(), 10_000_000);
        assert!(s.mask.bitmap_serialized_bytes() < 8192);
    }
    #[test]
    fn rectangle_matches_independent_item_truth_including_partial_last_row() {
        for columns in [1, 5, 11, 20] {
            for count in [1, 17, 143] {
                for a in [(0., 0.), (90., 100.), (-8., -40.)] {
                    for b in [(360., 420.), (21., 35.), (1300., 4000.)] {
                        let band = GridBand::from_rect(columns, 100., count, a, b);
                        let expected: Vec<_> = (0..count)
                            .filter(|id| {
                                let (x, y) =
                                    ((id % columns) as f64 * 108., (id / columns) as f64 * 128.);
                                a.0.min(b.0) < x + 100.
                                    && a.0.max(b.0) > x
                                    && a.1.min(b.1) < y + 112.
                                    && a.1.max(b.1) > y
                            })
                            .collect();
                        assert_eq!(
                            (0..count)
                                .filter(|id| band.contains(*id))
                                .collect::<Vec<_>>(),
                            expected
                        );
                        assert_eq!(band.len(), expected.len());
                        assert_eq!(band.first_id(), expected.first().copied());
                        assert_eq!(band.last_id(), expected.last().copied());
                    }
                }
            }
        }
    }
    #[test]
    fn additive_band_preserves_all_exceptions_and_disjoint_items() {
        let mut s = Selection::default();
        s.all(1000);
        s.select(5, true, false);
        s.select(900, true, false);
        let band = GridBand {
            first_row: 0,
            last_row: 2,
            first_col: 0,
            last_col: 10,
            columns: 10,
            count: 1000,
        };
        let result = s.mask.with_band(band, true, || false).unwrap();
        assert_eq!(result.count(), 999);
        assert!(result.contains(5));
        assert!(!result.contains(900));
        let mut s = Selection::default();
        s.select(900, false, false);
        let result = s.mask.with_band(band, true, || false).unwrap();
        assert_eq!(result.count(), 21);
        assert!(result.contains(900));
    }
    #[test]
    fn cancelled_bitmap_build_cannot_commit_a_partial_selection() {
        let band = GridBand {
            first_row: 0,
            last_row: 900_000,
            first_col: 1,
            last_col: 5,
            columns: 11,
            count: 10_000_000,
        };
        assert!(
            SelectionMask::default()
                .with_band(band, false, || true)
                .is_none()
        );
    }
    #[test]
    fn worker_wakes_after_idle_and_delivers_the_latest_requested_band() {
        let (tx, rx) = async_channel::bounded(1);
        let worker = SelectionWorker::new(tx);
        let wait = |token| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if let Ok(result) = rx.try_recv()
                    && result.token == token
                {
                    return result.mask;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "worker did not deliver latest selection"
                );
                thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        let small = GridBand {
            first_row: 0,
            last_row: 1,
            first_col: 0,
            last_col: 5,
            columns: 11,
            count: 10_000_000,
        };
        let token = worker.request(SelectionMask::default(), small, false);
        assert_eq!(wait(token).count(), 5);
        let expensive = GridBand {
            last_row: 900_000,
            ..small
        };
        for _ in 0..100 {
            worker.request(SelectionMask::default(), expensive, false);
        }
        worker.cancel();
        let token = worker.request(SelectionMask::default(), small, false);
        assert_eq!(wait(token).count(), 5);
    }
    #[test]
    fn ten_million_partial_column_selection_has_exact_count_and_bounded_encoding() {
        let band = GridBand {
            first_row: 0,
            last_row: 909_091,
            first_col: 1,
            last_col: 5,
            columns: 11,
            count: 10_000_000,
        };
        let start = std::time::Instant::now();
        let result = SelectionMask::default()
            .with_band(band, false, || false)
            .unwrap();
        assert_eq!(result.count(), 3_636_364);
        for id in [1, 4, 12, 15, 9_999_991, 9_999_994] {
            assert!(result.contains(id));
        }
        for id in [0, 5, 11, 16, 9_999_990, 9_999_999] {
            assert!(!result.contains(id));
        }
        println!(
            "partial selection: {} items, {} bitmap serialized bytes, {:.3} ms (CPU model only)",
            result.count(),
            result.bitmap_serialized_bytes(),
            start.elapsed().as_secs_f64() * 1000.
        );
        assert!(result.bitmap_serialized_bytes() < 2 * 1024 * 1024);
    }
}
