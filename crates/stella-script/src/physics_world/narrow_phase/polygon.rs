//! b2CollidePolygons (`sub_10085F648`) reference-face and manifold assembly.

mod clipping;
mod separation;

use super::geometry::native_normalize_if_ordered_at_least_epsilon_f32;
#[cfg(test)]
use crate::NativePolygon;
use crate::{
    BOX2D_POLYGON_RADIUS, ContactLocalManifold, ContactManifold, ContactManifoldType, ContactPoint,
    NativeToiTransform, contact_feature_id, swap_contact_features,
};

pub(super) use clipping::{ClipVertex, clip_segment_to_line};
#[cfg(test)]
pub(crate) use separation::polygon_max_separation;
pub(super) use separation::polygon_normals_f32;
use separation::{polygon_incident_edge_at_transforms, polygon_max_separation_at_transforms};

#[cfg(test)]
pub(crate) fn polygon_manifold(
    first: &[(f64, f64)],
    second: &[(f64, f64)],
) -> Option<ContactManifold> {
    let first = first
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    let second = second
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    polygon_manifold_at_transforms(
        &first,
        NativeToiTransform::IDENTITY,
        &second,
        NativeToiTransform::IDENTITY,
    )
}

pub(crate) fn polygon_manifold_at_transforms(
    first: &[(f32, f32)],
    first_transform: NativeToiTransform,
    second: &[(f32, f32)],
    second_transform: NativeToiTransform,
) -> Option<ContactManifold> {
    if first.len() < 3 || second.len() < 3 {
        return None;
    }
    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    let (first_separation, first_edge, first_normal) =
        polygon_max_separation_at_transforms(first, first_transform, second, second_transform)?;
    if first_separation > total_radius {
        return None;
    }
    let (second_separation, second_edge, second_normal) =
        polygon_max_separation_at_transforms(second, second_transform, first, first_transform)?;
    if second_separation > total_radius {
        return None;
    }

    // sub_10085F648 loads Box2D's exact k_absoluteTol=0.001f and
    // k_relativeTol=0.98f constants from 0x100A0C7CC/0x100A0C7D0.
    let (
        reference,
        reference_transform,
        incident,
        incident_transform,
        reference_edge,
        _separation_normal,
        manifold_type,
        flip,
    ) = if second_separation > first_separation.mul_add(0.98_f32, 0.001_f32) {
        (
            second,
            second_transform,
            first,
            first_transform,
            second_edge,
            second_normal,
            ContactManifoldType::FaceSecond,
            true,
        )
    } else {
        (
            first,
            first_transform,
            second,
            second_transform,
            first_edge,
            first_normal,
            ContactManifoldType::FaceFirst,
            false,
        )
    };
    let reference_start = reference[reference_edge];
    let reference_end = reference[(reference_edge + 1) % reference.len()];
    let tangent = native_normalize_if_ordered_at_least_epsilon_f32((
        reference_end.0 - reference_start.0,
        reference_end.1 - reference_start.1,
    ));
    // sub_10085F8DC..0x10085F920 derives the stored normal from the local
    // reference edge, rather than rotating the world-space SAT normal back.
    let local_normal = (tangent.1, -tangent.0);
    let world_tangent = reference_transform.rotate(tangent);
    let reference_normal = (world_tangent.1, -world_tangent.0);
    let reference_world_start = reference_transform.point(reference_start);
    let reference_world_end = reference_transform.point(reference_end);
    let (incident_index, incident_edge) = polygon_incident_edge_at_transforms(
        incident,
        incident_transform,
        local_normal,
        reference_transform,
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
        (-world_tangent.0, -world_tangent.1),
        -(world_tangent.0.mul_add(
            reference_world_start.0,
            world_tangent.1 * reference_world_start.1,
        )) + total_radius,
        reference_edge,
    );
    if clipped.len() < 2 {
        return None;
    }
    clipped = clip_segment_to_line(
        [clipped[0], clipped[1]],
        world_tangent,
        world_tangent.0.mul_add(
            reference_world_end.0,
            world_tangent.1 * reference_world_end.1,
        ) + total_radius,
        (reference_edge + 1) % reference.len(),
    );
    if clipped.is_empty() {
        return None;
    }

    let front_offset = reference_normal.0.mul_add(
        reference_world_start.0,
        reference_normal.1 * reference_world_start.1,
    );
    let mut points = clipped
        .into_iter()
        .filter_map(|vertex| {
            let point = vertex.point;
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
                incident_transform.inverse_point(point),
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
    let primary = points[0].0;
    let secondary = points.get(1).map(|point| point.0);
    let normal = if flip {
        (
            f64::from(-reference_normal.0),
            f64::from(-reference_normal.1),
        )
    } else {
        (f64::from(reference_normal.0), f64::from(reference_normal.1))
    };
    Some(ContactManifold {
        normal_x: normal.0,
        normal_y: normal.1,
        penetration: primary.penetration,
        point_x: primary.point_x,
        point_y: primary.point_y,
        feature_id: primary.feature_id,
        secondary,
        position: ContactLocalManifold {
            manifold_type,
            local_normal,
            local_point: (
                (reference_start.0 + reference_end.0) * 0.5_f32,
                (reference_start.1 + reference_end.1) * 0.5_f32,
            ),
            local_points,
            point_count,
            first_radius: BOX2D_POLYGON_RADIUS as f32,
            second_radius: BOX2D_POLYGON_RADIUS as f32,
        },
    })
}
