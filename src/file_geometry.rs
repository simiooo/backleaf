//! Painting and pointer hit testing share the same measured identity bounds.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.left + self.width && y >= self.top && y < self.top + self.height
    }
    pub fn rounded_contains(self, x: f32, y: f32, radius: f32) -> bool {
        if !self.contains(x, y) {
            return false;
        }
        let r = radius.min(self.width / 2.).min(self.height / 2.).max(0.);
        let dx = ((x - self.left - self.width / 2.).abs() - (self.width / 2. - r)).max(0.);
        let dy = ((y - self.top - self.height / 2.).abs() - (self.height / 2. - r)).max(0.);
        dx * dx + dy * dy <= r * r
    }
}
pub fn surface(cell_width: f32, name_width: f32, name_height: f32) -> Rect {
    let l = &crate::layout::LAYOUT;
    let width = (l.grid.icon_size.max(name_width) + l.file.visual_padding_x * 2.).min(cell_width);
    let height = l.file.visual_padding_top
        + l.grid.icon_size
        + l.file.icon_name_gap
        + name_height
        + l.file.visual_padding_bottom;
    Rect {
        left: (cell_width - width) / 2.,
        top: 0.,
        width,
        height,
    }
}
pub fn caption(cell_width: f32, name_width: f32, name_height: f32) -> Rect {
    let l = &crate::layout::LAYOUT;
    Rect {
        left: (cell_width - name_width) / 2.,
        top: l.file.visual_padding_top + l.grid.icon_size + l.file.icon_name_gap,
        width: name_width,
        height: name_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_identity_excludes_cell_whitespace_and_rounded_corners() {
        let s = surface(112., 32., 20.);
        assert_eq!(s.width, 60.);
        assert_eq!(s.height, 92.);
        assert!(s.rounded_contains(56., 30., 10.));
        assert!(s.rounded_contains(56., 70., 10.));
        for (x, y) in [
            (2., 30.),
            (110., 30.),
            (56., 105.),
            (s.left + 0.1, 0.1),
            (f32::NAN, 20.),
        ] {
            assert!(!s.rounded_contains(x, y, 10.));
        }
    }
    #[test]
    fn long_two_line_names_keep_their_visible_edges_clickable() {
        let s = surface(112., 96., 36.);
        assert_eq!(s.width, 112.);
        assert_eq!(s.height, 108.);
        assert!(s.rounded_contains(2., 70., 10.));
        assert!(!s.rounded_contains(56., 110., 10.));
        let smaller = surface(100., 84., 36.);
        assert_eq!(smaller.width, 100.);
        assert_eq!(smaller.height, s.height);
    }
    #[test]
    fn tooltip_region_is_caption_only_and_scales_with_measured_text() {
        let c = caption(112., 32., 20.);
        assert!(c.contains(56., 70.));
        assert!(!c.contains(56., 30.));
        assert!(!c.contains(10., 70.));
        assert!(!c.contains(56., 90.));
        let long = caption(100., 84., 36.);
        assert!(long.contains(10., 90.));
    }
}
