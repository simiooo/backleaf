//! Time-based edge scrolling in logical coordinates; independent of pointer event frequency.
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    edge_zone_minimum: f64,
    edge_zone_maximum: f64,
    edge_zone_fraction: f64,
    fine_screens_minimum: f64,
    fine_screens_maximum: f64,
    fine_ramp_gain: f64,
    fine_ramp_delay: f64,
    fine_ramp_duration: f64,
    deep_enter: f64,
    deep_leave: f64,
    range_ramp_delay: f64,
    range_ramp_duration: f64,
    range_traversal_seconds: f64,
    boundary_brake_seconds: f64,
    maximum_step_seconds: f64,
    reset_after_gap_seconds: f64,
}
static CONFIG: LazyLock<Settings> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../design/marquee-scroll-system.json"))
        .expect("marquee scroll calibration")
});
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edge {
    pub direction: i8,
    pub depth: f64,
}
pub fn edge_at(pointer: f64, top: f64, bottom: f64, offset: f64, maximum: f64) -> Edge {
    if ![pointer, top, bottom, offset, maximum]
        .iter()
        .all(|v| v.is_finite())
        || bottom <= top
        || maximum <= 0.
    {
        return Edge::default();
    }
    let c = &*CONFIG;
    let zone = ((bottom - top) * c.edge_zone_fraction)
        .clamp(c.edge_zone_minimum, c.edge_zone_maximum)
        .min((bottom - top) / 2.);
    if pointer < top + zone && offset > 0. {
        Edge {
            direction: -1,
            depth: ((top + zone - pointer) / zone).clamp(0., 1.),
        }
    } else if pointer > bottom - zone && offset < maximum {
        Edge {
            direction: 1,
            depth: ((pointer - bottom + zone) / zone).clamp(0., 1.),
        }
    } else {
        Edge::default()
    }
}
fn smooth(value: f64) -> f64 {
    let t = value.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Default, Debug)]
pub struct Controller {
    direction: i8,
    dwell: f64,
    deep_dwell: f64,
    deep: bool,
}
impl Controller {
    pub fn direction(&self) -> i8 {
        self.direction
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn observe(&mut self, edge: Edge) {
        if !matches!(edge.direction, -1 | 1) || !edge.depth.is_finite() || edge.depth <= 0. {
            self.reset();
            return;
        }
        if self.direction != edge.direction {
            self.reset();
            self.direction = edge.direction;
        }
        if self.deep && edge.depth < CONFIG.deep_leave {
            self.deep = false;
            self.deep_dwell = 0.;
        } else if !self.deep && edge.depth >= CONFIG.deep_enter {
            self.deep = true;
            self.deep_dwell = 0.;
        }
    }
    pub fn velocity(&self, edge: Edge, viewport: f64, maximum: f64, offset: f64) -> f64 {
        if !matches!(edge.direction, -1 | 1)
            || !edge.depth.is_finite()
            || ![viewport, maximum, offset].iter().all(|n| n.is_finite())
            || viewport <= 0.
            || maximum <= 0.
        {
            return 0.;
        }
        let p = edge.depth.clamp(0., 1.);
        if p == 0. {
            return 0.;
        }
        let c = &*CONFIG;
        let current = self.direction == edge.direction;
        let dwell = if current { self.dwell } else { 0. };
        let fine = viewport
            * (c.fine_screens_minimum
                + (c.fine_screens_maximum - c.fine_screens_minimum) * p * p
                + c.fine_ramp_gain
                    * p.powi(4)
                    * smooth((dwell - c.fine_ramp_delay) / c.fine_ramp_duration));
        let coarse = if current && self.deep && p >= c.deep_leave {
            let ceiling = ((c.fine_screens_maximum + c.fine_ramp_gain) * viewport)
                .max(maximum / c.range_traversal_seconds);
            let depth = ((p - c.deep_leave) / (1. - c.deep_leave)).clamp(0., 1.);
            (ceiling - fine).max(0.)
                * smooth((self.deep_dwell - c.range_ramp_delay) / c.range_ramp_duration)
                * depth
                * depth
        } else {
            0.
        };
        let remaining = if edge.direction > 0 {
            maximum - offset
        } else {
            offset
        };
        if remaining <= 0. {
            return 0.;
        }
        (fine + coarse)
            .min((remaining / c.boundary_brake_seconds).max(viewport * c.fine_screens_minimum))
            * f64::from(edge.direction)
    }
    pub fn advance(
        &mut self,
        edge: Edge,
        elapsed: f64,
        viewport: f64,
        maximum: f64,
        offset: f64,
    ) -> f64 {
        if !elapsed.is_finite() || elapsed <= 0. {
            return offset;
        }
        if elapsed > CONFIG.reset_after_gap_seconds {
            self.reset();
        }
        self.observe(edge);
        if self.direction == 0 {
            return offset;
        }
        let duration = if elapsed > CONFIG.reset_after_gap_seconds {
            CONFIG.maximum_step_seconds
        } else {
            elapsed
        };
        self.dwell += duration;
        if self.deep {
            self.deep_dwell += duration;
        }
        let velocity = self.velocity(edge, viewport, maximum, offset);
        if velocity == 0. {
            return offset;
        }
        let remaining = if velocity > 0. {
            maximum - offset
        } else {
            offset
        };
        if remaining <= 0.5 {
            return if velocity > 0. { maximum } else { 0. };
        }
        (offset + velocity * elapsed.min(CONFIG.maximum_step_seconds)).clamp(0., maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DOWN: Edge = Edge {
        direction: 1,
        depth: 1.,
    };
    fn run(seconds: f64, fps: u32, depth: f64) -> (Controller, f64) {
        let mut state = Controller::default();
        let mut offset = 0.;
        for _ in 0..(seconds * f64::from(fps)) as usize {
            offset = state.advance(
                Edge {
                    direction: 1,
                    depth,
                },
                1. / f64::from(fps),
                640.,
                160_000_000.,
                offset,
            );
        }
        (state, offset)
    }
    #[test]
    fn deep_residence_accelerates_large_ranges_and_shallow_stays_precise() {
        let (initial, _) = run(0.2, 120, 1.);
        let (fast, _) = run(8., 120, 1.);
        let (fine, _) = run(30., 120, 0.2);
        assert!(initial.velocity(DOWN, 640., 160_000_000., 0.) <= 641.);
        assert!(fast.velocity(DOWN, 640., 160_000_000., 0.) > 5_000_000.);
        assert!(
            fine.velocity(
                Edge {
                    direction: 1,
                    depth: 0.2
                },
                640.,
                160_000_000.,
                0.
            ) < 200.
        );
    }
    #[test]
    fn return_and_reverse_forget_acceleration_immediately() {
        let (mut fast, offset) = run(8., 120, 1.);
        fast.observe(Edge::default());
        assert_eq!(
            fast.advance(Edge::default(), 0.01, 640., 160_000_000., offset),
            offset
        );
        let up = Edge {
            direction: -1,
            depth: 1.,
        };
        assert_eq!(fast.velocity(up, 640., 160_000_000., offset), -640.);
        let end = fast.advance(up, 0.01, 640., 160_000_000., offset);
        assert!(offset - end <= 6.5);
    }
    #[test]
    fn mouse_wiggles_do_not_advance_the_clock() {
        let (mut a, _) = run(4., 120, 1.);
        let mut b = Controller::default();
        for _ in 0..50_000 {
            b.observe(DOWN);
        }
        assert_eq!(b.velocity(DOWN, 640., 160_000_000., 0.), 640.);
        let before = a.velocity(DOWN, 640., 160_000_000., 0.);
        for _ in 0..50_000 {
            a.observe(DOWN);
        }
        assert_eq!(a.velocity(DOWN, 640., 160_000_000., 0.), before);
    }
    #[test]
    fn integration_is_consistent_across_frame_rates() {
        let (_, a) = run(10., 60, 1.);
        let (_, b) = run(10., 120, 1.);
        assert!((a - b).abs() / b < 0.01, "{a} {b}");
    }
    #[test]
    fn long_frame_gap_restarts_fine_without_catching_up() {
        let (mut state, offset) = run(8., 120, 1.);
        let next = state.advance(DOWN, 5., 640., 160_000_000., offset);
        assert!(next - offset <= 33.);
    }
    #[test]
    fn endpoints_are_symmetric_and_never_overshoot() {
        let (mut state, _) = run(8., 120, 1.);
        assert_eq!(state.advance(DOWN, 0.01, 640., 10_000., 9999.8), 10_000.);
        let up = Edge {
            direction: -1,
            depth: 1.,
        };
        assert_eq!(state.advance(up, 0.01, 640., 10_000., 0.2), 0.);
        assert_eq!(edge_at(720., 64., 704., 10_000., 10_000.).direction, 0);
        assert_eq!(edge_at(0., 64., 704., 0., 10_000.).direction, 0);
    }
    #[test]
    fn screen_edge_and_invalid_inputs_do_not_depend_on_outside_room() {
        assert_eq!(edge_at(704., 64., 704., 0., 1_000_000.), DOWN);
        assert_eq!(
            edge_at(64., 64., 704., 1000., 1_000_000.),
            Edge {
                direction: -1,
                depth: 1.
            }
        );
        assert_eq!(
            edge_at(f64::NAN, 64., 704., 0., 1_000_000.),
            Edge::default()
        );
        assert_eq!(edge_at(20., 64., 20., 0., 10.), Edge::default());
    }
    #[test]
    fn deep_hysteresis_does_not_turn_shallow_residence_into_instant_high_speed() {
        let (mut state, offset) = run(30., 120, 0.3);
        state.observe(DOWN);
        assert!(state.velocity(DOWN, 640., 160_000_000., offset) < 6000.);
        for _ in 0..960 {
            state.advance(DOWN, 1. / 120., 640., 160_000_000., offset);
        }
        state.observe(Edge {
            direction: 1,
            depth: 0.4,
        });
        assert!(
            state.velocity(
                Edge {
                    direction: 1,
                    depth: 0.4
                },
                640.,
                160_000_000.,
                offset
            ) < 500.
        );
    }
}
