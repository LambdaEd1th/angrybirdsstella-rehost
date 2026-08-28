//! b2CollidePolygons (`sub_10085F648`) reference-face and manifold assembly.

mod clipping;
mod separation;

use super::geometry::normalized_axis_f32;
use crate::{
    BOX2D_POLYGON_RADIUS, ContactManifold, ContactPoint, ContactPositionState,
    ContactPositionWitness, contact_feature_id, swap_contact_features,
};

pub(super) use clipping::{ClipVertex, clip_segment_to_line};
pub(super) use separation::polygon_incident_edge;
pub(crate) use separation::polygon_max_separation;

pub(crate) fn polygon_manifold(
    first: &[(f64, f64)],
    second: &[(f64, f64)],
) -> Option<ContactManifold> {
    if first.len() < 3 || second.len() < 3 {
        return None;
    }
    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    let (first_separation, first_edge, first_normal) = polygon_max_separation(first, second)?;
    if first_separation as f32 > total_radius {
        return None;
    }
    let (second_separation, second_edge, second_normal) = polygon_max_separation(second, first)?;
    if second_separation as f32 > total_radius {
        return None;
    }

    // sub_10085F648 loads Box2D's exact k_absoluteTol=0.001f and
    // k_relativeTol=0.98f constants from 0x100A0C7CC/0x100A0C7D0.
    let (reference, incident, reference_edge, reference_normal, flip) =
        if second_separation as f32 > (first_separation as f32).mul_add(0.98_f32, 0.001_f32) {
            (second, first, second_edge, second_normal, true)
        } else {
            (first, second, first_edge, first_normal, false)
        };
    let reference_start = (
        reference[reference_edge].0 as f32,
        reference[reference_edge].1 as f32,
    );
    let reference_end = (
        reference[(reference_edge + 1) % reference.len()].0 as f32,
        reference[(reference_edge + 1) % reference.len()].1 as f32,
    );
    let tangent = normalized_axis_f32((
        reference_end.0 - reference_start.0,
        reference_end.1 - reference_start.1,
    ))?;
    let reference_normal = (reference_normal.0 as f32, reference_normal.1 as f32);
    let (incident_index, incident_edge) = polygon_incident_edge(
        incident,
        (f64::from(reference_normal.0), f64::from(reference_normal.1)),
    )?;
    let incident_vertices = [
        ClipVertex {
            point: incident_edge.0,
            feature_id: contact_feature_id(reference_edge, incident_index, 1, 0),
        },
        ClipVertex {
            point: incident_edge.1,
            feature_id: contact_feature_id(
                reference_edge,
                (incident_index + 1) % incident.len(),
                1,
                0,
            ),
        },
    ];
    let mut clipped = clip_segment_to_line(
        incident_vertices,
        (f64::from(-tangent.0), f64::from(-tangent.1)),
        f64::from(
            -(tangent
                .0
                .mul_add(reference_start.0, tangent.1 * reference_start.1))
                + total_radius,
        ),
        reference_edge,
    );
    if clipped.len() < 2 {
        return None;
    }
    clipped = clip_segment_to_line(
        [clipped[0], clipped[1]],
        (f64::from(tangent.0), f64::from(tangent.1)),
        f64::from(
            tangent
                .0
                .mul_add(reference_end.0, tangent.1 * reference_end.1)
                + total_radius,
        ),
        (reference_edge + 1) % reference.len(),
    );
    if clipped.is_empty() {
        return None;
    }

    let front_offset = reference_normal
        .0
        .mul_add(reference_start.0, reference_normal.1 * reference_start.1);
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
                    feature_id: if flip {
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
    let normal = if flip {
        (
            f64::from(-reference_normal.0),
            f64::from(-reference_normal.1),
        )
    } else {
        (f64::from(reference_normal.0), f64::from(reference_normal.1))
    };
    points.truncate(2);
    let clip_points = [
        points[0].1,
        points.get(1).map(|point| point.1).unwrap_or((0.0, 0.0)),
    ];
    let point_count = points.len() as u8;
    let primary = points[0].0;
    let secondary = points.get(1).map(|point| point.0);
    let plane_point = (
        (reference_start.0 + reference_end.0) * 0.5_f32,
        (reference_start.1 + reference_end.1) * 0.5_f32,
    );
    Some(ContactManifold {
        normal_x: normal.0,
        normal_y: normal.1,
        penetration: primary.penetration,
        point_x: primary.point_x,
        point_y: primary.point_y,
        feature_id: primary.feature_id,
        secondary,
        position: ContactPositionState::World(if flip {
            ContactPositionWitness::FaceSecond {
                normal: reference_normal,
                plane_point,
                clip_points,
                point_count,
                first_radius: BOX2D_POLYGON_RADIUS as f32,
                second_radius: BOX2D_POLYGON_RADIUS as f32,
            }
        } else {
            ContactPositionWitness::FaceFirst {
                normal: reference_normal,
                plane_point,
                clip_points,
                point_count,
                first_radius: BOX2D_POLYGON_RADIUS as f32,
                second_radius: BOX2D_POLYGON_RADIUS as f32,
            }
        }),
    })
}
