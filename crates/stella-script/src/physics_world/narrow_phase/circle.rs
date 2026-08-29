//! Circle-circle and polygon-circle narrow-phase members.

use super::geometry::{native_arm_le_f32, native_arm_lt_f32, native_fmax_f32};
#[cfg(test)]
use crate::NativePolygon;
use crate::{
    BOX2D_POLYGON_RADIUS, ContactLocalManifold, ContactManifold, ContactManifoldType,
    NativeToiTransform, native_polygon_normals,
};

#[cfg(test)]
pub(crate) fn circle_circle_manifold(
    first_center: (f64, f64),
    first_radius: f64,
    second_center: (f64, f64),
    second_radius: f64,
) -> Option<ContactManifold> {
    circle_circle_manifold_at_transforms(
        (first_center.0 as f32, first_center.1 as f32),
        first_radius as f32,
        NativeToiTransform::IDENTITY,
        (second_center.0 as f32, second_center.1 as f32),
        second_radius as f32,
        NativeToiTransform::IDENTITY,
    )
}

pub(crate) fn circle_circle_manifold_at_transforms(
    first_local_center: (f32, f32),
    first_radius: f32,
    first_transform: NativeToiTransform,
    second_local_center: (f32, f32),
    second_radius: f32,
    second_transform: NativeToiTransform,
) -> Option<ContactManifold> {
    let first_center = first_transform.point(first_local_center);
    let second_center = second_transform.point(second_local_center);
    let delta = (
        second_center.0 - first_center.0,
        second_center.1 - first_center.1,
    );
    // sub_10085E590 squares the packed delta and reduces it with faddp.
    let distance_squared = delta.0 * delta.0 + delta.1 * delta.1;
    let radius_sum = first_radius + second_radius;
    if distance_squared > radius_sum * radius_sum {
        return None;
    }
    // b2CollideCircles uses the packed FMUL/FADDP distance above only for its
    // overlap decision, then emits two local centers. b2WorldManifold
    // deliberately recomputes the world delta with scalar FMUL/FMADD before
    // normalizing it, so the packed distance cannot be reused for world data.
    let local_manifold = ContactManifold {
        normal_x: 0.0,
        normal_y: 0.0,
        penetration: 0.0,
        point_x: 0.0,
        point_y: 0.0,
        feature_id: 0,
        secondary: None,
        position: ContactLocalManifold {
            manifold_type: ContactManifoldType::Circles,
            local_normal: (0.0, 0.0),
            local_point: first_local_center,
            local_points: [second_local_center, (0.0, 0.0)],
            point_count: 1,
            first_radius,
            second_radius,
        },
    };
    Some(local_manifold.at_native_transforms(first_transform, second_transform))
}

#[cfg(test)]
pub(crate) fn circle_polygon_manifold(
    circle_center: (f64, f64),
    radius: f64,
    polygon: &[(f64, f64)],
    circle_is_first: bool,
) -> Option<ContactManifold> {
    let polygon = polygon
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    circle_polygon_manifold_at_transforms(
        (circle_center.0 as f32, circle_center.1 as f32),
        radius as f32,
        NativeToiTransform::IDENTITY,
        &polygon,
        NativeToiTransform::IDENTITY,
        circle_is_first,
    )
}

pub(crate) fn circle_polygon_manifold_at_transforms(
    circle_local_center: (f32, f32),
    circle_radius: f32,
    circle_transform: NativeToiTransform,
    polygon: &[(f32, f32)],
    polygon_transform: NativeToiTransform,
    circle_is_first: bool,
) -> Option<ContactManifold> {
    if polygon.len() < 3 {
        return None;
    }
    let circle_world_center = circle_transform.point(circle_local_center);
    let circle_center = polygon_transform.inverse_point(circle_world_center);
    let polygon_radius = BOX2D_POLYGON_RADIUS as f32;
    let total_radius = circle_radius + polygon_radius;
    let normals = native_polygon_normals(polygon);
    let mut face_index = 0;
    let mut face_separation = -f32::MAX;
    for index in 0..polygon.len() {
        let start = polygon[index];
        let normal = normals[index];
        // sub_10085E624 uses a vector multiply followed by faddp here,
        // so both products round before the addition rather than fusing.
        let separation =
            (circle_center.0 - start.0) * normal.0 + (circle_center.1 - start.1) * normal.1;
        if separation > total_radius {
            return None;
        }
        let replaces_face = separation > face_separation;
        face_separation = native_fmax_f32(separation, face_separation);
        if replaces_face {
            face_index = index;
        }
    }
    // 0x10085E6BC updates the running scalar with FMAX independently from
    // the ordered-GT index selection at 0x10085E6C0..0x10085E6C4. The
    // selected normal is loaded only after the scan, even when the first
    // candidate was unordered and therefore did not replace index zero.
    let face_normal = normals[face_index];

    let first_vertex = polygon[face_index];
    let second_vertex = polygon[(face_index + 1) % polygon.len()];
    let face_center = (
        (first_vertex.0 + second_vertex.0) * 0.5_f32,
        (first_vertex.1 + second_vertex.1) * 0.5_f32,
    );
    let edge = (
        second_vertex.0 - first_vertex.0,
        second_vertex.1 - first_vertex.1,
    );
    let reverse_edge = (
        first_vertex.0 - second_vertex.0,
        first_vertex.1 - second_vertex.1,
    );
    let first_delta = (
        circle_center.0 - first_vertex.0,
        circle_center.1 - first_vertex.1,
    );
    let first_region = first_delta.0.mul_add(edge.0, first_delta.1 * edge.1);
    let second_delta = (
        circle_center.0 - second_vertex.0,
        circle_center.1 - second_vertex.1,
    );
    let second_region = second_delta
        .0
        .mul_add(reverse_edge.0, second_delta.1 * reverse_edge.1);

    // This is b2CollidePolygonAndCircle's face/vertex region selection, not
    // a generic SAT support point. The latter chooses an arbitrary extreme
    // polygon vertex along the face normal and produces the wrong lever arm
    // for an edge contact (large enough to destabilize closed joint loops).
    // The native epsilon branch classifies centers within the polygon as a
    // face contact before testing either vertex Voronoi region.
    let face_region = native_arm_lt_f32(face_separation, f32::EPSILON);
    let first_vertex_region = native_arm_le_f32(first_region, 0.0_f32);
    let second_vertex_region = native_arm_le_f32(second_region, 0.0_f32);
    let (polygon_to_circle, _separation, plane_point) = if face_region {
        (face_normal, face_separation, face_center)
    } else if first_vertex_region {
        let distance_squared = first_delta
            .0
            .mul_add(first_delta.0, first_delta.1 * first_delta.1);
        if distance_squared > total_radius * total_radius {
            return None;
        }
        let distance = distance_squared.sqrt();
        let normal = native_polygon_circle_vertex_normal(first_delta, distance);
        (normal, distance, first_vertex)
    } else if second_vertex_region {
        let distance_squared = second_delta
            .0
            .mul_add(second_delta.0, second_delta.1 * second_delta.1);
        if distance_squared > total_radius * total_radius {
            return None;
        }
        let distance = distance_squared.sqrt();
        let normal = native_polygon_circle_vertex_normal(second_delta, distance);
        (normal, distance, second_vertex)
    } else {
        let relative = (
            circle_center.0 - face_center.0,
            circle_center.1 - face_center.1,
        );
        let separation = relative
            .0
            .mul_add(face_normal.0, relative.1 * face_normal.1);
        if separation > total_radius {
            return None;
        }
        (face_normal, separation, face_center)
    };

    let position = if circle_is_first {
        ContactLocalManifold {
            manifold_type: ContactManifoldType::FaceSecond,
            local_normal: polygon_to_circle,
            local_point: plane_point,
            local_points: [circle_local_center, (0.0, 0.0)],
            point_count: 1,
            first_radius: circle_radius,
            second_radius: polygon_radius,
        }
    } else {
        ContactLocalManifold {
            manifold_type: ContactManifoldType::FaceFirst,
            local_normal: polygon_to_circle,
            local_point: plane_point,
            local_points: [circle_local_center, (0.0, 0.0)],
            point_count: 1,
            first_radius: polygon_radius,
            second_radius: circle_radius,
        }
    };
    // b2CollidePolygonAndCircle finishes with the local face manifold. Let
    // b2WorldManifold derive penetration from its independently rounded world
    // surfaces instead of collapsing it to totalRadius - planeDistance.
    let local_manifold = ContactManifold {
        normal_x: 0.0,
        normal_y: 0.0,
        penetration: 0.0,
        point_x: 0.0,
        point_y: 0.0,
        // Every branch of sub_10085E624 clears b2ManifoldPoint::id.key.
        feature_id: 0,
        secondary: None,
        position,
    };
    let (first_transform, second_transform) = if circle_is_first {
        (circle_transform, polygon_transform)
    } else {
        (polygon_transform, circle_transform)
    };
    Some(local_manifold.at_native_transforms(first_transform, second_transform))
}

fn native_polygon_circle_vertex_normal(delta: (f32, f32), distance: f32) -> (f32, f32) {
    if distance >= f32::EPSILON {
        // 0x10085E834..0x10085E844 and 0x10085E884..0x10085E894
        // divide 1.0 by the distance once, then multiply both lanes. Two
        // direct component divisions can differ from this by one ULP.
        let inverse_distance = distance.recip();
        (delta.0 * inverse_distance, delta.1 * inverse_distance)
    } else {
        // Purple retains a zero, sub-epsilon or unordered delta rather than
        // falling back to the adjacent face normal.
        delta
    }
}
