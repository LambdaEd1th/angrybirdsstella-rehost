//! b2EPCollider edge-polygon leaf (`sub_10085EADC`).

use super::super::{
    NativePolygon,
    geometry::{native_normalize_or_preserve_f32, normalized_axis_f32, polygon_centroid_f32},
    polygon::{ClipVertex, clip_segment_to_line, polygon_normals_f32},
};
use crate::{
    BOX2D_POLYGON_RADIUS, ContactLocalManifold, ContactManifold, ContactManifoldType, ContactPoint,
    NativeToiTransform, contact_feature_id, swap_contact_features,
};

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
    let polygon_centroid = relative_transform.point(polygon_centroid_f32(polygon_local)?);
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
        separation.min(edge_normal.0 * delta.0 + edge_normal.1 * delta.1)
    });
    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    if edge_separation > total_radius {
        return None;
    }

    let mut polygon_axis = (-f32::MAX, 0_usize, (0.0_f32, 0.0_f32));
    let edge_perpendicular = (-edge_normal.1, edge_normal.0);
    let lower_limit = (-edge_normal.0, -edge_normal.1);
    let upper_limit = lower_limit;
    let angular_slop = f32::from_bits(0x3D0E_FA36);
    for &(index, normal) in &polygon_normals {
        let vertex = polygon[index];
        let start_delta = (edge_start.0 - vertex.0, edge_start.1 - vertex.1);
        let end_delta = (edge_end.0 - vertex.0, edge_end.1 - vertex.1);
        let separation = (normal.0 * start_delta.0 + normal.1 * start_delta.1)
            .min(normal.0 * end_delta.0 + normal.1 * end_delta.1);
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
        if edge_normal.0.mul_add(
            candidate_normal.0 - limit.0,
            edge_normal.1 * (candidate_normal.1 - limit.1),
        ) < -angular_slop
        {
            continue;
        }
        if separation > polygon_axis.0 {
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
        let mut start = polygon[face_index];
        let mut end = polygon[next_index];
        let mut start_index = face_index;
        let mut end_index = next_index;
        let tangent = normalized_axis_f32((end.0 - start.0, end.1 - start.1))?;
        if tangent
            .1
            .mul_add(polygon_axis.2.0, -(tangent.0 * polygon_axis.2.1))
            < 0.0_f32
        {
            std::mem::swap(&mut start, &mut end);
            std::mem::swap(&mut start_index, &mut end_index);
        }
        (
            start,
            end,
            start_index,
            end_index,
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
            if alignment < incident_alignment {
                incident_alignment = alignment;
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

    let reference_tangent = native_normalize_or_preserve_f32((
        reference_end.0 - reference_start.0,
        reference_end.1 - reference_start.1,
    ));
    let first_offset = -(reference_tangent
        .0
        .mul_add(reference_start.0, reference_tangent.1 * reference_start.1))
        + total_radius;
    let mut clipped = clip_segment_to_line(
        incident,
        (-reference_tangent.0, -reference_tangent.1),
        first_offset,
        reference_start_index,
    );
    if clipped.len() < 2 {
        return None;
    }
    let second_offset = reference_tangent
        .0
        .mul_add(reference_end.0, reference_tangent.1 * reference_end.1)
        + total_radius;
    clipped = clip_segment_to_line(
        [clipped[0], clipped[1]],
        reference_tangent,
        second_offset,
        reference_end_index,
    );
    if clipped.is_empty() {
        return None;
    }

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
            let contact_edge = (
                (-0.5_f32 * separation).mul_add(reference_normal.0, point.0),
                (-0.5_f32 * separation).mul_add(reference_normal.1, point.1),
            );
            let contact_world = segment_transform.point(contact_edge);
            let incident_local = if reference_is_polygon {
                point
            } else {
                relative_transform.inverse_point(point)
            };
            (separation <= total_radius).then_some((
                ContactPoint {
                    penetration: f64::from(total_radius - separation),
                    point_x: f64::from(contact_world.0),
                    point_y: f64::from(contact_world.1),
                    feature_id: if swap_features {
                        swap_contact_features(vertex.feature_id)
                    } else {
                        vertex.feature_id
                    },
                },
                incident_local,
            ))
        })
        .collect::<Vec<_>>();
    if points.is_empty() {
        return None;
    }
    points.truncate(2);
    let normal_edge = if polygon_is_first {
        if reference_is_polygon {
            reference_normal
        } else {
            (-reference_normal.0, -reference_normal.1)
        }
    } else if reference_is_polygon {
        (-reference_normal.0, -reference_normal.1)
    } else {
        reference_normal
    };
    let normal = segment_transform.rotate(normal_edge);
    let local_points = [
        points[0].1,
        points.get(1).map(|point| point.1).unwrap_or((0.0, 0.0)),
    ];
    let point_count = points.len() as u8;
    let primary = points[0].0;
    let secondary = points.get(1).map(|point| point.0);
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
    Some(ContactManifold {
        normal_x: f64::from(normal.0),
        normal_y: f64::from(normal.1),
        penetration: primary.penetration,
        point_x: primary.point_x,
        point_y: primary.point_y,
        feature_id: primary.feature_id,
        secondary,
        position: ContactLocalManifold {
            manifold_type,
            local_normal,
            local_point,
            local_points,
            point_count,
            first_radius: BOX2D_POLYGON_RADIUS as f32,
            second_radius: BOX2D_POLYGON_RADIUS as f32,
        },
    })
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
