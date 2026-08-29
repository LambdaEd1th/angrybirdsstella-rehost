//! Shared float32 geometry helpers used by Purple's narrow phase.

#[cfg(test)]
use crate::cross_2d;

#[cfg(test)]
pub(crate) fn polygon_contains_point(polygon: &[(f64, f64)], point: (f64, f64)) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..polygon.len() {
        let first = polygon[index];
        let second = polygon[(index + 1) % polygon.len()];
        let edge = (second.0 - first.0, second.1 - first.1);
        let relative = (point.0 - first.0, point.1 - first.1);
        let cross = cross_2d(edge, relative);
        let projection = dot_2d(relative, edge);
        if cross.abs() <= 1e-9 && projection >= -1e-9 && projection <= dot_2d(edge, edge) + 1e-9 {
            return true;
        }
        let crosses = (first.1 > point.1) != (second.1 > point.1);
        if crosses
            && point.0 < first.0 + (point.1 - first.1) * (second.0 - first.0) / (second.1 - first.1)
        {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
pub(crate) fn dot_2d(first: (f64, f64), second: (f64, f64)) -> f64 {
    first.0 * second.0 + first.1 * second.1
}

pub(crate) fn normalized_axis_f32(axis: (f32, f32)) -> Option<(f32, f32)> {
    let length_squared = axis.0.mul_add(axis.0, axis.1 * axis.1);
    if length_squared <= f32::EPSILON * f32::EPSILON {
        return None;
    }
    let inverse_length = length_squared.sqrt().recip();
    Some((axis.0 * inverse_length, axis.1 * inverse_length))
}

/// Match the reference-edge normalization in `b2CollidePolygons` at
/// `0x10085F88C..0x10085F8C8`. Its `FCMP`/`B.GE` reciprocal path is taken
/// only for an ordered length at least `FLT_EPSILON`; zero, sub-epsilon and
/// unordered NaN tangents retain their raw lanes.
pub(crate) fn native_normalize_if_ordered_at_least_epsilon_f32(axis: (f32, f32)) -> (f32, f32) {
    let length = axis.0.mul_add(axis.0, axis.1 * axis.1).sqrt();
    if length >= f32::EPSILON {
        let inverse_length = length.recip();
        (axis.0 * inverse_length, axis.1 * inverse_length)
    } else {
        axis
    }
}

/// Match b2Vec2::Normalize as inlined by b2EPCollider::Collide. Purple forms
/// the length in float32, skips only an *ordered* value below FLT_EPSILON,
/// and otherwise multiplies by its reciprocal. Zero/sub-epsilon vectors are
/// therefore preserved rather than rejected, while NaN follows the divide
/// path because ARM's unordered comparison does not take B.LT.
pub(crate) fn native_normalize_or_preserve_f32(axis: (f32, f32)) -> (f32, f32) {
    let length = axis.0.mul_add(axis.0, axis.1 * axis.1).sqrt();
    if length < f32::EPSILON {
        axis
    } else {
        let inverse_length = length.recip();
        (axis.0 * inverse_length, axis.1 * inverse_length)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        native_normalize_if_ordered_at_least_epsilon_f32, native_normalize_or_preserve_f32,
    };

    #[test]
    fn polygon_reference_normalize_preserves_zero_and_sub_epsilon_vectors() {
        for axis in [(0.0, -0.0_f32), (f32::EPSILON * 0.25, -0.0_f32)] {
            let normalized = native_normalize_if_ordered_at_least_epsilon_f32(axis);

            assert_eq!(normalized.0.to_bits(), axis.0.to_bits());
            assert_eq!(normalized.1.to_bits(), axis.1.to_bits());
        }
    }

    #[test]
    fn polygon_reference_normalize_scales_the_epsilon_boundary() {
        let normalized = native_normalize_if_ordered_at_least_epsilon_f32((f32::EPSILON, 0.0));

        assert_eq!(normalized.0.to_bits(), 1.0_f32.to_bits());
        assert_eq!(normalized.1.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn polygon_reference_normalize_preserves_unordered_raw_lanes() {
        let normalized = native_normalize_if_ordered_at_least_epsilon_f32((f32::NAN, 1.0));

        assert!(normalized.0.is_nan());
        assert_eq!(normalized.1.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn native_normalize_preserves_ordered_sub_epsilon_vectors() {
        let axis = (f32::EPSILON * 0.25, -0.0_f32);
        let normalized = native_normalize_or_preserve_f32(axis);

        assert_eq!(normalized.0.to_bits(), axis.0.to_bits());
        assert_eq!(normalized.1.to_bits(), axis.1.to_bits());
    }

    #[test]
    fn native_normalize_scales_the_epsilon_boundary() {
        let normalized = native_normalize_or_preserve_f32((f32::EPSILON, 0.0));

        assert_eq!(normalized.0.to_bits(), 1.0_f32.to_bits());
        assert_eq!(normalized.1.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn native_normalize_sends_unordered_length_through_reciprocal() {
        let normalized = native_normalize_or_preserve_f32((f32::NAN, 1.0));

        assert!(normalized.0.is_nan());
        assert!(normalized.1.is_nan());
    }
}
