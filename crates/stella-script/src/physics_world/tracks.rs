//! Native chain-track state and closest-edge projection.

use std::cmp::Ordering;

fn native_track_fraction(value: f32) -> f32 {
    // sub_10085DA8C..94 uses FMIN(value, 1) followed by FMAX(value, +0).
    // Preserve the unordered operand and the native +0 result for -0.
    let upper = if value.is_nan() || value < 1.0_f32 {
        value
    } else {
        1.0_f32
    };
    if upper.is_nan() || upper > 0.0_f32 {
        upper
    } else {
        0.0_f32
    }
}

fn native_track_distance_squared(delta: (f32, f32)) -> f32 {
    // sub_10085D4B4..B8 squares both SIMD lanes before FADDP. It does not
    // fuse either product into the other lane.
    let x_squared = delta.0 * delta.0;
    let y_squared = delta.1 * delta.1;
    x_squared + y_squared
}

fn native_track_candidate_is_better(distance: f32, best: f32) -> bool {
    // FCMP/B.GE skips only ordered greater/equal values. Unordered values
    // fall through to the candidate write at 0x10085D4C4.
    !matches!(
        distance.partial_cmp(&best),
        Some(Ordering::Equal | Ordering::Greater)
    )
}

#[derive(Debug, Clone)]
pub(crate) struct ClosestTrackEdge {
    pub(crate) index: i32,
    pub(crate) start: (f64, f64),
    pub(crate) end: (f64, f64),
    pub(crate) angle: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct PhysicsTrack {
    pub(crate) object: String,
    pub(crate) points: Vec<(f64, f64)>,
    pub(crate) _open_ended: bool,
    pub(crate) rotate_block: bool,
    pub(crate) current_segment: i32,
    pub(crate) edge_start: (f64, f64),
    pub(crate) edge_end: (f64, f64),
    pub(crate) angle: f64,
    pub(crate) impulse_x: f64,
    pub(crate) impulse_y: f64,
}

impl PhysicsTrack {
    pub(crate) fn native_closest_edge(&self, position: (f64, f64)) -> Option<ClosestTrackEdge> {
        let point_x = position.0 as f32;
        let point_y = position.1 as f32;
        let mut best_distance = f32::INFINITY;
        let mut best_edge = None::<ClosestTrackEdge>;
        for (index, edge) in self.points.windows(2).enumerate() {
            let start_x = edge[0].0 as f32;
            let start_y = edge[0].1 as f32;
            let end_x = edge[1].0 as f32;
            let end_y = edge[1].1 as f32;
            let delta_x = end_x - start_x;
            let delta_y = end_y - start_y;
            let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
            let projection = native_track_fraction(
                (point_x - start_x).mul_add(delta_x, (point_y - start_y) * delta_y)
                    / length_squared,
            );
            let closest_x = delta_x.mul_add(projection, start_x);
            let closest_y = delta_y.mul_add(projection, start_y);
            let distance_x = point_x - closest_x;
            let distance_y = point_y - closest_y;
            let distance_squared = native_track_distance_squared((distance_x, distance_y));
            // Finite equal distances retain the first child, while the
            // FCMP/B.GE unordered path still writes the current candidate.
            if native_track_candidate_is_better(distance_squared, best_distance) {
                best_distance = distance_squared;
                best_edge = Some(ClosestTrackEdge {
                    index: index as i32,
                    start: (f64::from(start_x), f64::from(start_y)),
                    end: (f64::from(end_x), f64::from(end_y)),
                    angle: f64::from(delta_y.atan2(delta_x)),
                });
            }
        }
        best_edge
    }

    pub(crate) fn native_current_angle(&self, position: (f64, f64)) -> f64 {
        self.native_closest_edge(position)
            .map(|edge| edge.angle)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PhysicsTrack, native_track_candidate_is_better, native_track_distance_squared,
        native_track_fraction,
    };

    #[test]
    fn closest_edge_distance_squares_both_lanes_before_native_faddp() {
        let delta = (f32::from_bits(0x3FD4_7B66), f32::from_bits(0x3F59_DB8D));
        let native = native_track_distance_squared(delta);
        let formerly_fused = delta.0.mul_add(delta.0, delta.1 * delta.1);

        assert_eq!(native.to_bits(), 0x405E_B618);
        assert_eq!(formerly_fused.to_bits(), 0x405E_B619);
    }

    #[test]
    fn closest_edge_comparison_keeps_ties_but_accepts_unordered_candidate() {
        assert!(!native_track_candidate_is_better(2.0, 2.0));
        assert!(!native_track_candidate_is_better(3.0, 2.0));
        assert!(native_track_candidate_is_better(1.0, 2.0));
        assert!(native_track_candidate_is_better(f32::NAN, 2.0));
        assert!(native_track_candidate_is_better(2.0, f32::NAN));
    }

    #[test]
    fn closest_point_fraction_uses_native_fmin_fmax_zero_and_nan_rules() {
        assert_eq!(native_track_fraction(-0.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(native_track_fraction(-1.0), 0.0);
        assert_eq!(native_track_fraction(2.0), 1.0);
        assert!(native_track_fraction(f32::NAN).is_nan());
    }

    #[test]
    fn degenerate_edge_follows_native_unordered_candidate_write() {
        let track = PhysicsTrack {
            object: "track".to_owned(),
            points: vec![(0.0, 0.0), (2.0, 0.0), (2.0, 0.0)],
            _open_ended: true,
            rotate_block: false,
            current_segment: -1,
            edge_start: (0.0, 0.0),
            edge_end: (0.0, 0.0),
            angle: 0.0,
            impulse_x: 0.0,
            impulse_y: 0.0,
        };

        let closest = track.native_closest_edge((1.0, 1.0)).unwrap();

        assert_eq!(closest.index, 1);
        assert_eq!(closest.angle.to_bits(), 0.0_f64.to_bits());
    }
}
