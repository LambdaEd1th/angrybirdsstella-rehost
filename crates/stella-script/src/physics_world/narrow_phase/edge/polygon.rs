//! b2EPCollider edge-polygon leaf (`sub_10085EADC`).

use super::super::{
    NativePolygon,
    geometry::{
        native_arm_le_f32, native_arm_lt_f32, native_fmin_f32, native_normalize_or_preserve_f32,
    },
    polygon::{ClipVertex, clip_segment_to_line_pair, polygon_normals_f32},
};
use crate::{
    BOX2D_POLYGON_RADIUS, ContactLocalManifold, ContactManifold, ContactManifoldType, ContactPoint,
    NativeToiTransform, contact_feature_id, native_polygon_centroid_f32, swap_contact_features,
};

const NATIVE_EDGE_ANGULAR_SLOP: f32 = f32::from_bits(0x3D0E_FA36);

#[cfg(test)]
pub(crate) fn polygon_segment_manifold(
    polygon: &[(f64, f64)],
    segment: ((f64, f64), (f64, f64)),
    polygon_is_first: bool,
) -> Option<ContactManifold> {
    let polygon = polygon
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    polygon_segment_manifold_at_transforms(
        &polygon,
        NativeToiTransform::IDENTITY,
        (
            (segment.0.0 as f32, segment.0.1 as f32),
            (segment.1.0 as f32, segment.1.1 as f32),
        ),
        NativeToiTransform::IDENTITY,
        polygon_is_first,
    )
}

pub(crate) fn polygon_segment_manifold_at_transforms(
    polygon_local: &[(f32, f32)],
    polygon_transform: NativeToiTransform,
    segment_local: ((f32, f32), (f32, f32)),
    segment_transform: NativeToiTransform,
    polygon_is_first: bool,
) -> Option<ContactManifold> {
    if polygon_local.len() < 3 {
        return None;
    }
    let relative_transform = polygon_to_edge_transform(polygon_transform, segment_transform);
    let polygon = polygon_local
        .iter()
        .copied()
        .map(|point| relative_transform.point(point))
        .collect::<NativePolygon<_>>();
    let polygon_local_normals = polygon_normals_f32(polygon_local)?;
    let polygon_normals = polygon_local_normals
        .iter()
        .copied()
        .enumerate()
        .map(|(index, normal)| (index, relative_transform.rotate(normal)))
        .collect::<NativePolygon<_>>();
    let edge_start = segment_local.0;
    let edge_end = segment_local.1;
    let edge_vector = (edge_end.0 - edge_start.0, edge_end.1 - edge_start.1);
    let edge_tangent = native_normalize_or_preserve_f32(edge_vector);
    let base_edge_normal = (edge_tangent.1, -edge_tangent.0);

    // With no adjacent vertices (the independent edge fixtures created by
    // Purple), sub_10085EADC chooses the edge side from the polygon centroid.
    let polygon_centroid = relative_transform.point(native_polygon_centroid_f32(polygon_local));
    let centroid_delta = (
        polygon_centroid.0 - edge_start.0,
        polygon_centroid.1 - edge_start.1,
    );
    let front = base_edge_normal
        .0
        .mul_add(centroid_delta.0, base_edge_normal.1 * centroid_delta.1)
        >= 0.0_f32;
    let edge_normal = if front {
        base_edge_normal
    } else {
        (-base_edge_normal.0, -base_edge_normal.1)
    };
    let edge_separation = polygon.iter().fold(f32::MAX, |separation, &point| {
        let delta = (point.0 - edge_start.0, point.1 - edge_start.1);
        let candidate = native_fmul_faddp_dot_f32(edge_normal, delta);
        native_edge_axis_min(candidate, separation)
    });
    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    if edge_separation > total_radius {
        return None;
    }

    let mut polygon_axis = (-f32::MAX, 0_usize, (0.0_f32, 0.0_f32));
    let edge_perpendicular = (-edge_normal.1, edge_normal.0);
    let lower_limit = (-edge_normal.0, -edge_normal.1);
    let upper_limit = lower_limit;
    for &(index, normal) in &polygon_normals {
        let vertex = polygon[index];
        let start_delta = (edge_start.0 - vertex.0, edge_start.1 - vertex.1);
        let end_delta = (edge_end.0 - vertex.0, edge_end.1 - vertex.1);
        let separation = native_fmin_f32(
            native_fmul_faddp_dot_f32(normal, start_delta),
            native_fmul_faddp_dot_f32(normal, end_delta),
        );
        if separation > total_radius {
            return None;
        }
        let candidate_normal = (-normal.0, -normal.1);
        let limit = if candidate_normal.0.mul_add(
            edge_perpendicular.0,
            candidate_normal.1 * edge_perpendicular.1,
        ) >= 0.0_f32
        {
            upper_limit
        } else {
            lower_limit
        };
        let normal_delta = (candidate_normal.0 - limit.0, candidate_normal.1 - limit.1);
        let angular_separation = native_fmul_faddp_dot_f32(edge_normal, normal_delta);
        if native_polygon_axis_improves(angular_separation, separation, polygon_axis.0) {
            polygon_axis = (separation, index, normal);
        }
    }

    // The native primary-axis hysteresis at 0x10085F248 uses the same
    // 0.98f/0.001f constants as polygon-polygon collision.
    let polygon_reference = polygon_axis.0 > edge_separation.mul_add(0.98_f32, 0.001_f32);
    let (
        reference_start,
        reference_end,
        reference_start_index,
        reference_end_index,
        reference_normal,
        reference_is_polygon,
        incident,
    ) = if polygon_reference {
        let face_index = polygon_axis.1;
        let next_index = (face_index + 1) % polygon.len();
        (
            polygon[face_index],
            polygon[next_index],
            face_index,
            next_index,
            polygon_axis.2,
            true,
            [
                ClipVertex {
                    point: edge_start,
                    feature_id: contact_feature_id(face_index, 0, 1, 0),
                },
                ClipVertex {
                    point: edge_end,
                    feature_id: contact_feature_id(face_index, 1, 1, 0),
                },
            ],
        )
    } else {
        let (start, end, start_index, end_index) = if front {
            (edge_start, edge_end, 0, 1)
        } else {
            (edge_end, edge_start, 1, 0)
        };
        let mut incident_index = 0;
        let mut incident_alignment = f32::MAX;
        for &(index, normal) in &polygon_normals {
            let alignment = normal.0.mul_add(edge_normal.0, normal.1 * edge_normal.1);
            let replaces_index = native_arm_lt_f32(alignment, incident_alignment);
            incident_alignment = native_fmin_f32(alignment, incident_alignment);
            if replaces_index {
                incident_index = index;
            }
        }
        let incident_edge = (
            polygon[incident_index],
            polygon[(incident_index + 1) % polygon.len()],
        );
        (
            start,
            end,
            start_index,
            end_index,
            edge_normal,
            false,
            [
                ClipVertex {
                    point: incident_edge.0,
                    feature_id: contact_feature_id(0, incident_index, 1, 0),
                },
                ClipVertex {
                    point: incident_edge.1,
                    feature_id: contact_feature_id(0, (incident_index + 1) % polygon.len(), 1, 0),
                },
            ],
        )
    };

    // 0x10085F2E8..0x10085F3F4 takes the selected stored normal and rotates
    // it into the two clipping side normals. It does not normalize the
    // reference edge or reorder a polygon-owned face here.
    let reference_tangent = (-reference_normal.1, reference_normal.0);
    let first_offset = -(reference_tangent
        .0
        .mul_add(reference_start.0, reference_tangent.1 * reference_start.1))
        + total_radius;
    let mut clipped = clip_segment_to_line_pair(
        incident,
        (-reference_tangent.0, -reference_tangent.1),
        first_offset,
        reference_start_index,
    )?;
    let second_offset = reference_tangent
        .0
        .mul_add(reference_end.0, reference_tangent.1 * reference_end.1)
        + total_radius;
    clipped = clip_segment_to_line_pair(
        clipped,
        reference_tangent,
        second_offset,
        reference_end_index,
    )?;

    let front_offset = reference_normal
        .0
        .mul_add(reference_start.0, reference_normal.1 * reference_start.1);
    let swap_features = reference_is_polygon != polygon_is_first;
    let mut points = clipped
        .into_iter()
        .filter_map(|vertex| {
            let point = vertex.point;
            let separation = reference_normal
                .0
                .mul_add(point.0, reference_normal.1 * point.1)
                - front_offset;
            let incident_local = if reference_is_polygon {
                point
            } else {
                relative_transform.inverse_point(point)
            };
            native_arm_le_f32(separation, total_radius).then_some((
                if swap_features {
                    swap_contact_features(vertex.feature_id)
                } else {
                    vertex.feature_id
                },
                incident_local,
            ))
        })
        .collect::<Vec<_>>();
    if points.is_empty() {
        return None;
    }
    points.truncate(2);
    let local_points = [
        points[0].1,
        points.get(1).map(|point| point.1).unwrap_or((0.0, 0.0)),
    ];
    let point_count = points.len() as u8;
    let manifold_type = match (reference_is_polygon, polygon_is_first) {
        (true, true) | (false, false) => ContactManifoldType::FaceFirst,
        (true, false) | (false, true) => ContactManifoldType::FaceSecond,
    };
    let (local_normal, local_point) = if reference_is_polygon {
        (
            polygon_local_normals[polygon_axis.1],
            polygon_local[polygon_axis.1],
        )
    } else {
        (reference_normal, reference_start)
    };
    let placeholder = |feature_id| ContactPoint {
        penetration: 0.0,
        point_x: 0.0,
        point_y: 0.0,
        feature_id,
    };
    let local_manifold = ContactManifold {
        normal_x: 0.0,
        normal_y: 0.0,
        penetration: 0.0,
        point_x: 0.0,
        point_y: 0.0,
        feature_id: points[0].0,
        secondary: points.get(1).map(|point| placeholder(point.0)),
        position: ContactLocalManifold {
            manifold_type,
            local_normal,
            local_point,
            local_points,
            point_count,
            first_radius: BOX2D_POLYGON_RADIUS as f32,
            second_radius: BOX2D_POLYGON_RADIUS as f32,
        },
    };
    let (first_transform, second_transform) = if polygon_is_first {
        (polygon_transform, segment_transform)
    } else {
        (segment_transform, polygon_transform)
    };
    Some(local_manifold.at_native_transforms(first_transform, second_transform))
}

fn polygon_to_edge_transform(
    polygon_transform: NativeToiTransform,
    edge_transform: NativeToiTransform,
) -> NativeToiTransform {
    let position_delta = (
        polygon_transform.position.0 - edge_transform.position.0,
        polygon_transform.position.1 - edge_transform.position.1,
    );
    NativeToiTransform {
        position: edge_transform.inverse_rotate(position_delta),
        sine: polygon_transform.sine.mul_add(
            edge_transform.cosine,
            -(polygon_transform.cosine * edge_transform.sine),
        ),
        cosine: polygon_transform.sine.mul_add(
            edge_transform.sine,
            polygon_transform.cosine * edge_transform.cosine,
        ),
    }
}

/// Match the packed `FMUL` followed by `FADDP` used by the edge-polygon axis
/// scans. Both products round before the horizontal addition.
fn native_fmul_faddp_dot_f32(first: (f32, f32), second: (f32, f32)) -> f32 {
    let product_x = first.0 * second.0;
    let product_y = first.1 * second.1;
    product_x + product_y
}

/// `0x10085F0B4` uses `FMIN candidate, running`; when both lanes are NaN,
/// the current candidate's payload replaces the prior accumulator payload.
fn native_edge_axis_min(candidate: f32, running: f32) -> f32 {
    native_fmin_f32(candidate, running)
}

/// `0x10085F16C..0x10085F174` updates only when the angular limit comparison
/// is ordered greater/equal and the candidate separation is ordered greater.
fn native_polygon_axis_improves(
    angular_separation: f32,
    candidate_separation: f32,
    running_separation: f32,
) -> bool {
    angular_separation >= -NATIVE_EDGE_ANGULAR_SLOP && candidate_separation > running_separation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_axis_fmin_keeps_the_latest_nan_candidate_payload() {
        let first = f32::from_bits(0x7FC1_2345);
        let second = f32::from_bits(0xFFC5_4321);
        let running = native_edge_axis_min(first, f32::MAX);
        let running = native_edge_axis_min(second, running);

        assert_eq!(running.to_bits(), second.to_bits());
    }

    #[test]
    fn packed_axis_dot_rounds_both_products_before_faddp() {
        let first = (f32::from_bits(0x42FB_DBF2), f32::from_bits(0x40CD_2D26));
        let second = (f32::from_bits(0xC30B_7EFE), f32::from_bits(0x4419_C605));
        let native = native_fmul_faddp_dot_f32(first, second);
        let fused = first.0.mul_add(second.0, first.1 * second.1);

        assert_eq!(native.to_bits(), 0xC654_DB62);
        assert_eq!(fused.to_bits(), 0xC654_DB61);
    }

    #[test]
    fn polygon_axis_gate_rejects_unordered_angle_or_separation() {
        assert!(native_polygon_axis_improves(
            -NATIVE_EDGE_ANGULAR_SLOP,
            1.0,
            0.0,
        ));
        assert!(!native_polygon_axis_improves(f32::NAN, 1.0, 0.0));
        assert!(!native_polygon_axis_improves(0.0, f32::NAN, 0.0));
        assert!(!native_polygon_axis_improves(0.0, 0.0, 0.0));
    }
}
