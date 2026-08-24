//! b2ClipSegmentToLine (`sub_100860148`).

use crate::contact_feature_id;

#[derive(Debug, Clone, Copy)]
pub(in crate::physics_world::narrow_phase) struct ClipVertex {
    pub(in crate::physics_world::narrow_phase) point: (f64, f64),
    pub(in crate::physics_world::narrow_phase) feature_id: u32,
}

pub(in crate::physics_world::narrow_phase) fn clip_segment_to_line(
    segment: [ClipVertex; 2],
    normal: (f64, f64),
    offset: f64,
    reference_vertex: usize,
) -> Vec<ClipVertex> {
    let normal = (normal.0 as f32, normal.1 as f32);
    let offset = offset as f32;
    let first_point = (segment[0].point.0 as f32, segment[0].point.1 as f32);
    let second_point = (segment[1].point.0 as f32, segment[1].point.1 as f32);
    let first_distance = normal.0.mul_add(first_point.0, normal.1 * first_point.1) - offset;
    let second_distance = normal.0.mul_add(second_point.0, normal.1 * second_point.1) - offset;
    let mut output = Vec::with_capacity(2);
    if first_distance <= 0.0_f32 {
        output.push(segment[0]);
    }
    if second_distance <= 0.0_f32 {
        output.push(segment[1]);
    }
    if first_distance * second_distance < 0.0_f32 {
        let fraction = first_distance / (first_distance - second_distance);
        let incident_index = ((segment[0].feature_id >> 8) & 0xff) as usize;
        output.push(ClipVertex {
            point: (
                f64::from(fraction.mul_add(second_point.0 - first_point.0, first_point.0)),
                f64::from(fraction.mul_add(second_point.1 - first_point.1, first_point.1)),
            ),
            feature_id: contact_feature_id(reference_vertex, incident_index, 0, 1),
        });
    }
    output.truncate(2);
    output
}
