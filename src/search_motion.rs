//! Finite search-field motion; retargeting always starts at the visible width.
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct SearchMotion {
    from: f32,
    target: f32,
    started: Option<Instant>,
    duration: Duration,
}
impl SearchMotion {
    pub fn value(&self, now: Instant, reduced: bool) -> f32 {
        if reduced || self.duration.is_zero() {
            return self.target;
        }
        let elapsed = self
            .started
            .map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
        let t = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0., 1.);
        self.from + (self.target - self.from) * (1. - (1. - t).powi(3))
    }
    pub fn retarget(&mut self, open: bool, now: Instant, full_duration: Duration) {
        let target = if open { 1. } else { 0. };
        if self.target == target {
            return;
        }
        self.from = self.value(now, false);
        self.target = target;
        self.started = Some(now);
        self.duration = full_duration.mul_f32((target - self.from).abs());
    }
    pub fn animating(&self, now: Instant) -> bool {
        !self.duration.is_zero()
            && self
                .started
                .is_some_and(|t| now.saturating_duration_since(t) < self.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_and_closing_are_bounded_monotonic_and_finite() {
        let start = Instant::now();
        let mut motion = SearchMotion::default();
        assert!(!motion.animating(start));
        for (open, offset, duration) in [(true, 0, 240), (false, 300, 200)] {
            let now = start + Duration::from_millis(offset);
            motion.retarget(open, now, Duration::from_millis(duration));
            let mut last = motion.value(now, false);
            for ms in 1..=duration + 20 {
                let value = motion.value(now + Duration::from_millis(ms), false);
                assert!((0. ..=1.).contains(&value));
                assert!(if open { value >= last } else { value <= last });
                last = value;
            }
            assert_eq!(last, if open { 1. } else { 0. });
            assert!(!motion.animating(now + Duration::from_millis(duration)));
        }
    }
    #[test]
    fn interruption_keeps_the_current_width_and_repeated_close_does_not_restart() {
        let start = Instant::now();
        let mut motion = SearchMotion::default();
        motion.retarget(true, start, Duration::from_millis(240));
        let now = start + Duration::from_millis(60);
        let visible = motion.value(now, false);
        assert!(visible > 0.5);
        motion.retarget(false, now, Duration::from_millis(200));
        assert_eq!(motion.value(now, false), visible);
        let again = now + Duration::from_millis(20);
        let visible = motion.value(again, false);
        motion.retarget(false, again, Duration::from_millis(200));
        assert_eq!(motion.value(again, false), visible);
        motion.retarget(true, again, Duration::from_millis(240));
        assert_eq!(motion.value(again, false), visible);
        assert_eq!(motion.value(again, true), 1.);
        motion.retarget(false, again, Duration::from_millis(200));
        assert_eq!(motion.value(again, true), 0.);
        assert_eq!(motion.value(again + Duration::from_secs(1), false), 0.);
    }
}
