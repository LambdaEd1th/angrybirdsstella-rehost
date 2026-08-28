//! b2EPCollider edge-polygon leaf (`sub_10085EADC`).

use super::super::{
    NativePolygon,
    geometry::{normalized_axis_f32, polygon_centroid_f32, polygon_signed_area_from_f32},
    polygon::{ClipVertex, clip_segment_to_line, polygon_incident_edge},
};
use crate::{
    BOX2D_POLYGON_RADIUS, ContactManifold, ContactManifoldType, ContactPoint, ContactPositionState,
    ContactPositionWitness, contact_feature_id, swap_contact_features,
};

pub(crate) fn polygon_segment_manifold(
    polygon: &[(f64, f64)],
    segment: ((f64, f64), (f64, f64)),
    polygon_is_first: bool,
) -> Option<ContactManifold> {
    if polygon.len() < 3 {
        return None;
    }
    let polygon = polygon
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    let edge_start = (segment.0.0 as f32, segment.0.1 as f32);
    let edge_end = (segment.1.0 as f32, segment.1.1 as f32);
    let edge_vector = (edge_end.0 - edge_start.0, edge_end.1 - edge_start.1);
    let edge_tangent = normalized_axis_f32(edge_vector)?;
    let base_edge_normal = (edge_tangent.1, -edge_tangent.0);
    let polygon_orientation = polygon_signed_area_from_f32(&polygon);
    let polygon_normals = polygon
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = polygon[(index + 1) % polygon.len()];
            let edge = (end.0 - start.0, end.1 - start.1);
            let outward = if polygon_orientation >= 0.0_f32 {
                (edge.1, -edge.0)
            } else {
                (-edge.1, edge.0)
            };
            normalized_axis_f32(outward).map(|normal| (index, normal))
        })
        .collect::<Option<NativePolygon<_>>>()?;

    // With no adjacent vertices (the independent edge fixtures created by
    // Purple), sub_10085EADC chooses the edge side from the polygon centroid.
    let polygon_centroid = polygon_centroid_f32(&polygon)?;
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
    for &(index, normal) in &polygon_normals {
        let vertex = polygon[index];
        let start_delta = (edge_start.0 - vertex.0, edge_start.1 - vertex.1);
        let end_delta = (edge_end.0 - vertex.0, edge_end.1 - vertex.1);
        let separation = normal
            .0
            .mul_add(start_delta.0, normal.1 * start_delta.1)
            .min(normal.0.mul_add(end_delta.0, normal.1 * end_delta.1));
        if separation > polygon_axis.0 {
            polygon_axis = (separation, index, normal);
        }
    }
    if polygon_axis.0 > total_radius {
        return None;
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
                    point: (f64::from(edge_start.0), f64::from(edge_start.1)),
                    feature_id: contact_feature_id(face_index, 0, 1, 0),
                },
                ClipVertex {
                    point: (f64::from(edge_end.0), f64::from(edge_end.1)),
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
        let polygon_f64 = polygon
            .iter()
            .map(|&(x, y)| (f64::from(x), f64::from(y)))
            .collect::<NativePolygon<_>>();
        let (incident_index, incident_edge) = polygon_incident_edge(
            &polygon_f64,
            (f64::from(edge_normal.0), f64::from(edge_normal.1)),
        )?;
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

    let reference_tangent = normalized_axis_f32((
        reference_end.0 - reference_start.0,
        reference_end.1 - reference_start.1,
    ))?;
    let first_offset = -(reference_tangent
        .0
        .mul_add(reference_start.0, reference_tangent.1 * reference_start.1))
        + total_radius;
    let mut clipped = clip_segment_to_line(
        incident,
        (
            f64::from(-reference_tangent.0),
            f64::from(-reference_tangent.1),
        ),
        f64::from(first_offset),
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
        (
            f64::from(reference_tangent.0),
            f64::from(reference_tangent.1),
        ),
        f64::from(second_offset),
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
            let point = (vertex.point.0 as f32, vertex.point.1 as f32);
            let separation = reference_normal
                .0
                .mul_add(point.0, reference_normal.1 * point.1)
                - front_offset;
            (separation <= total_radius).then_some((
                ContactPoint {
                    penetration: f64::from(total_radius - separation),
                    point_x: f64::from(
                        (-0.5_f32 * separation).mul_add(reference_normal.0, point.0),
                    ),
                    point_y: f64::from(
                        (-0.5_f32 * separation).mul_add(reference_normal.1, point.1),
                    ),
                    feature_id: if swap_features {
                        swap_contact_features(vertex.feature_id)
                    } else {
                        vertex.feature_id
                    },
                },
                point,
            ))
        })
        .collect::<Vec<_>>();
    if points.is_empty() {
        return None;
    }
    points.truncate(2);
    let normal = if polygon_is_first {
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
    let clip_points = [
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
    let position_witness = if matches!(manifold_type, ContactManifoldType::FaceFirst) {
        ContactPositionWitness::FaceFirst {
            normal: reference_normal,
            plane_point: reference_start,
            clip_points,
            point_count,
            first_radius: BOX2D_POLYGON_RADIUS as f32,
            second_radius: BOX2D_POLYGON_RADIUS as f32,
        }
    } else {
        ContactPositionWitness::FaceSecond {
            normal: reference_normal,
            plane_point: reference_start,
            clip_points,
            point_count,
            first_radius: BOX2D_POLYGON_RADIUS as f32,
            second_radius: BOX2D_POLYGON_RADIUS as f32,
        }
    };
    Some(ContactManifold {
        normal_x: f64::from(normal.0),
        normal_y: f64::from(normal.1),
        penetration: primary.penetration,
        point_x: primary.point_x,
        point_y: primary.point_y,
        feature_id: primary.feature_id,
        secondary,
        position: ContactPositionState::World(position_witness),
    })
}
