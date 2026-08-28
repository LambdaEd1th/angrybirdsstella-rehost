//! b2ClipSegmentToLine (`sub_100860148`).

use crate::contact_feature_id;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy)]
pub(in crate::physics_world::narrow_phase) struct ClipVertex {
    pub(in crate::physics_world::narrow_phase) point: (f32, f32),
    pub(in crate::physics_world::narrow_phase) feature_id: u32,
}

pub(in crate::physics_world::narrow_phase) fn clip_segment_to_line(
    segment: [ClipVertex; 2],
    normal: (f32, f32),
    offset: f32,
    reference_vertex: usize,
) -> SmallVec<[ClipVertex; 2]> {
    let first_point = segment[0].point;
    let second_point = segment[1].point;
    let first_dot = normal.0.mul_add(first_point.0, normal.1 * first_point.1);
    let second_dot = normal.0.mul_add(second_point.0, normal.1 * second_point.1);
    let first_distance = first_dot - offset;
    let second_distance = second_dot - offset;
    let mut output = SmallVec::new();
    if first_distance <= 0.0_f32 {
        output.push(segment[0]);
    }
    if second_distance <= 0.0_f32 {
        output.push(segment[1]);
    }
    if first_distance * second_distance < 0.0_f32 {
        // sub_1008601C8 subtracts the two un-offset dot products. Reusing
        // distance1-distance2 introduces two extra f32 rounding boundaries.
        let fraction = first_distance / (first_dot - second_dot);
        let incident_index = ((segment[0].feature_id >> 8) & 0xff) as usize;
        output.push(ClipVertex {
            point: (
                fraction.mul_add(second_point.0 - first_point.0, first_point.0),
                fraction.mul_add(second_point.1 - first_point.1, first_point.1),
            ),
            feature_id: contact_feature_id(reference_vertex, incident_index, 0, 1),
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_fraction_uses_native_unoffset_dot_denominator() {
        let first_feature = contact_feature_id(7, 13, 1, 0);
        let output = clip_segment_to_line(
            [
                ClipVertex {
                    point: (-44_141_752.0, 0.0),
                    feature_id: first_feature,
                },
                ClipVertex {
                    point: (27_333_470.0, 1.0),
                    feature_id: contact_feature_id(7, 14, 1, 0),
                },
            ],
            (1.0, 0.0),
            -11_625_316.0,
            5,
        );
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].feature_id, first_feature);
        assert_eq!(output[1].point.1.to_bits(), 0x3ee8_ecf9);
        assert_eq!(output[1].feature_id, contact_feature_id(5, 13, 0, 1));
    }
}
