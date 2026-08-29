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

/// Match Purple's inlined float32 vector normalization. Its reciprocal path
/// is entered only for an ordered length at least `FLT_EPSILON`; zero,
/// sub-epsilon and unordered NaN vectors retain their raw lanes.
pub(crate) fn native_normalize_or_preserve_f32(axis: (f32, f32)) -> (f32, f32) {
    let length = axis.0.mul_add(axis.0, axis.1 * axis.1).sqrt();
    if length >= f32::EPSILON {
        let inverse_length = length.recip();
        (axis.0 * inverse_length, axis.1 * inverse_length)
    } else {
        axis
    }
}

/// AArch64 `FCMP` followed by `B.LT`/`CSEL LT`: ordered less-than and
/// unordered both satisfy the condition because unordered sets NZCV=0011.
pub(crate) fn native_arm_lt_f32(left: f32, right: f32) -> bool {
    matches!(
        left.partial_cmp(&right),
        None | Some(std::cmp::Ordering::Less)
    )
}

/// AArch64 `FMIN`: propagate and quiet the first NaN operand, retain negative
/// zero, otherwise return the smaller ordered operand.
pub(crate) fn native_fmin_f32(left: f32, right: f32) -> f32 {
    if left.is_nan() {
        return f32::from_bits(left.to_bits() | 0x0040_0000);
    }
    if right.is_nan() {
        return f32::from_bits(right.to_bits() | 0x0040_0000);
    }
    if left == right {
        if left == 0.0_f32 && (left.is_sign_negative() || right.is_sign_negative()) {
            return -0.0_f32;
        }
        return left;
    }
    if left < right { left } else { right }
}

/// AArch64 `FMAX`: propagate and quiet the first NaN operand, retain positive
/// zero, otherwise return the larger ordered operand.
pub(crate) fn native_fmax_f32(left: f32, right: f32) -> f32 {
    if left.is_nan() {
        return f32::from_bits(left.to_bits() | 0x0040_0000);
    }
    if right.is_nan() {
        return f32::from_bits(right.to_bits() | 0x0040_0000);
    }
    if left == right {
        if left == 0.0_f32 && (!left.is_sign_negative() || !right.is_sign_negative()) {
            return 0.0_f32;
        }
        return left;
    }
    if left > right { left } else { right }
}

#[cfg(test)]
mod tests {
    use super::{
        native_arm_lt_f32, native_fmax_f32, native_fmin_f32, native_normalize_or_preserve_f32,
    };

    #[test]
    fn arm_lt_accepts_ordered_less_and_unordered_only() {
        assert!(native_arm_lt_f32(-1.0, 0.0));
        assert!(native_arm_lt_f32(f32::NAN, 0.0));
        assert!(!native_arm_lt_f32(0.0, 0.0));
        assert!(!native_arm_lt_f32(1.0, 0.0));
    }

    #[test]
    fn native_minmax_propagate_payloads_and_choose_signed_zero() {
        let first_nan = f32::from_bits(0x7F81_2345);
        let second_nan = f32::from_bits(0xFFC5_4321);

        assert_eq!(native_fmin_f32(first_nan, 5.0).to_bits(), 0x7FC1_2345);
        assert_eq!(native_fmax_f32(5.0, second_nan).to_bits(), 0xFFC5_4321);
        assert_eq!(
            native_fmin_f32(first_nan, second_nan).to_bits(),
            0x7FC1_2345
        );
        assert_eq!(native_fmin_f32(0.0, -0.0).to_bits(), (-0.0_f32).to_bits());
        assert_eq!(native_fmax_f32(-0.0, 0.0).to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn native_normalize_preserves_zero_and_sub_epsilon_vectors() {
        for axis in [(0.0, -0.0_f32), (f32::EPSILON * 0.25, -0.0_f32)] {
            let normalized = native_normalize_or_preserve_f32(axis);

            assert_eq!(normalized.0.to_bits(), axis.0.to_bits());
            assert_eq!(normalized.1.to_bits(), axis.1.to_bits());
        }
    }

    #[test]
    fn native_normalize_scales_the_epsilon_boundary() {
        let normalized = native_normalize_or_preserve_f32((f32::EPSILON, 0.0));

        assert_eq!(normalized.0.to_bits(), 1.0_f32.to_bits());
        assert_eq!(normalized.1.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn native_normalize_preserves_unordered_raw_lanes() {
        let normalized = native_normalize_or_preserve_f32((f32::NAN, 1.0));

        assert!(normalized.0.is_nan());
        assert_eq!(normalized.1.to_bits(), 1.0_f32.to_bits());
    }
}
