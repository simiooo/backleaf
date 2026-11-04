//! Scroll geometry stays in logical f64 coordinates, independent of virtual-list rebasing.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub viewport: f64,
    pub maximum: f64,
    pub offset: f64,
    pub thumb: f32,
    pub top: f32,
    pub travel: f32,
}
impl Geometry {
    pub fn new(viewport: f64, content: f64, offset: f64, minimum_thumb: f32) -> Option<Self> {
        if !viewport.is_finite()
            || !content.is_finite()
            || !offset.is_finite()
            || viewport <= 0.
            || content <= viewport + 0.5
        {
            return None;
        }
        let maximum = content - viewport;
        let offset = offset.clamp(0., maximum);
        let track = viewport as f32;
        let thumb = (viewport * viewport / content) as f32;
        let thumb = thumb.clamp(minimum_thumb.min(track), track);
        let travel = (track - thumb).max(0.);
        Some(Self {
            viewport,
            maximum,
            offset,
            thumb,
            travel,
            top: (offset / maximum) as f32 * travel,
        })
    }
    pub fn dragged_offset(self, delta: f32) -> f64 {
        if self.travel <= 0. {
            return self.offset;
        }
        (self.offset + f64::from(delta / self.travel) * self.maximum).clamp(0., self.maximum)
    }
}

/// Return a bounded local window and sub-row offset for any position in the full directory.
pub fn rebase(offset: f64, rows: usize, row_height: f64) -> (usize, f32) {
    let span = rows.min(4096);
    let row = (offset / row_height).floor().max(0.) as usize;
    let origin = row.saturating_sub(span / 2).min(rows.saturating_sub(span));
    (origin, (offset - origin as f64 * row_height) as f32)
}

/// Sidebar visibility must not depend on whether an overlaid child blocks its hitbox.
pub fn inside_sidebar(x: f32, y: f32, width: f32, header: f32, height: f32) -> bool {
    x >= 0. && x < width && y >= header && y < height
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidebar_hover_includes_its_overlay_and_excludes_other_panes() {
        assert!(inside_sidebar(178., 120., 184., 48., 440.));
        assert!(inside_sidebar(4., 200., 184., 48., 440.));
        for (x, y) in [
            (184., 120.),
            (30., 47.),
            (30., 440.),
            (-1., 120.),
            (f32::NAN, 120.),
        ] {
            assert!(!inside_sidebar(x, y, 184., 48., 440.));
        }
        assert!(!inside_sidebar(0., 120., 0., 48., 440.));
    }
    #[test]
    fn hidden_when_content_fits_or_container_is_empty() {
        for (viewport, content) in [(0., 400.), (400., 0.), (400., 400.), (400., 400.4)] {
            assert!(Geometry::new(viewport, content, 0., 24.).is_none());
        }
        assert!(Geometry::new(f64::NAN, 500., 0., 24.).is_none());
    }
    #[test]
    fn proportional_thumb_resizes_and_clamps_at_both_ends() {
        let short = Geometry::new(200., 800., -100., 24.).unwrap();
        let tall = Geometry::new(400., 800., 10000., 24.).unwrap();
        assert_eq!(short.thumb, 50.);
        assert_eq!(tall.thumb, 200.);
        assert_eq!(short.top, 0.);
        assert_eq!(tall.top, tall.travel);
        assert_eq!(short.dragged_offset(short.travel), short.maximum);
        let tiny = Geometry::new(12., 10000000., 0., 24.).unwrap();
        assert_eq!(tiny.thumb, 12.);
        assert!(tiny.dragged_offset(100.).is_finite());
    }
    #[test]
    fn million_row_drag_uses_full_range_and_preserves_sub_row_position() {
        let rows = 1_000_000;
        let geometry = Geometry::new(640., rows as f64 * 128., 0., 24.).unwrap();
        assert_eq!(geometry.thumb, 24.);
        let offset = geometry.dragged_offset(geometry.travel * 0.75) + 0.125;
        let (origin, local) = rebase(offset, rows, 128.);
        assert!(origin > 4096);
        assert!(local < 4096. * 128.);
        assert!((origin as f64 * 128. + f64::from(local) - offset).abs() < 0.02);
        let end = geometry.dragged_offset(geometry.travel);
        let (origin, local) = rebase(end, rows, 128.);
        assert_eq!(origin, rows - 4096);
        assert!((f64::from(local) + 640. - 4096. * 128.).abs() < 0.01);
    }
}
