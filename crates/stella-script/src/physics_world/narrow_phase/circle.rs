//! Circle-circle and polygon-circle narrow-phase members.

use super::geometry::{normalized_axis_f32, polygon_signed_area_f32};
use crate::{BOX2D_POLYGON_RADIUS, ContactManifold, ContactPositionState, ContactPositionWitness};

pub(crate) fn circle_circle_manifold(
    first_center: (f64, f64),
    first_radius: f64,
    second_center: (f64, f64),
    second_radius: f64,
) -> Option<ContactManifold> {
    let first_center = (first_center.0 as f32, first_center.1 as f32);
    let second_center = (second_center.0 as f32, second_center.1 as f32);
    let first_radius = first_radius as f32;
    let second_radius = second_radius as f32;
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
    let normal = if distance_squared > f32::EPSILON * f32::EPSILON {
        let inverse_distance = distance_squared.sqrt().recip();
        (delta.0 * inverse_distance, delta.1 * inverse_distance)
    } else {
        // b2WorldManifold's native circles branch retains this axis for
        // coincident and sub-epsilon centers.
        (1.0_f32, 0.0_f32)
    };
    let first_surface = (
        first_radius.mul_add(normal.0, first_center.0),
        first_radius.mul_add(normal.1, first_center.1),
    );
    let second_surface = (
        (-second_radius).mul_add(normal.0, second_center.0),
        (-second_radius).mul_add(normal.1, second_center.1),
    );
    let surface_delta = (
        second_surface.0 - first_surface.0,
        second_surface.1 - first_surface.1,
    );
    let separation = surface_delta
        .0
        .mul_add(normal.0, surface_delta.1 * normal.1);
    Some(ContactManifold {
        normal_x: f64::from(normal.0),
        normal_y: f64::from(normal.1),
        penetration: f64::from(-separation),
        point_x: f64::from((first_surface.0 + second_surface.0) * 0.5_f32),
        point_y: f64::from((first_surface.1 + second_surface.1) * 0.5_f32),
        feature_id: 0,
        secondary: None,
        position: ContactPositionState::World(ContactPositionWitness::Circles {
            first_center,
            second_center,
            first_radius,
            second_radius,
        }),
    })
}

pub(crate) fn circle_polygon_manifold(
    circle_center: (f64, f64),
    radius: f64,
    polygon: &[(f64, f64)],
    circle_is_first: bool,
) -> Option<ContactManifold> {
    if polygon.len() < 3 {
        return None;
    }
    let circle_center = (circle_center.0 as f32, circle_center.1 as f32);
    let circle_radius = radius as f32;
    let polygon_radius = BOX2D_POLYGON_RADIUS as f32;
    let total_radius = circle_radius + polygon_radius;
    let orientation = polygon_signed_area_f32(polygon);
    let mut face_index = 0;
    let mut face_separation = -f32::MAX;
    let mut face_normal = (0.0_f32, 0.0_f32);
    for index in 0..polygon.len() {
        let start = (polygon[index].0 as f32, polygon[index].1 as f32);
        let end = (
            polygon[(index + 1) % polygon.len()].0 as f32,
            polygon[(index + 1) % polygon.len()].1 as f32,
        );
        let edge = (end.0 - start.0, end.1 - start.1);
        let outward = if orientation >= 0.0_f32 {
            (edge.1, -edge.0)
        } else {
            (-edge.1, edge.0)
        };
        let normal = normalized_axis_f32(outward)?;
        // sub_10085E624 uses a vector multiply followed by faddp here,
        // so both products round before the addition rather than fusing.
        let separation =
            (circle_center.0 - start.0) * normal.0 + (circle_center.1 - start.1) * normal.1;
        if separation > total_radius {
            return None;
        }
        if separation > face_separation {
            face_separation = separation;
            face_index = index;
            face_normal = normal;
        }
    }

    let first_vertex = (polygon[face_index].0 as f32, polygon[face_index].1 as f32);
    let second_vertex = (
        polygon[(face_index + 1) % polygon.len()].0 as f32,
        polygon[(face_index + 1) % polygon.len()].1 as f32,
    );
    let face_center = (
        (first_vertex.0 + second_vertex.0) * 0.5_f32,
        (first_vertex.1 + second_vertex.1) * 0.5_f32,
    );
    let edge = (
        second_vertex.0 - first_vertex.0,
        second_vertex.1 - first_vertex.1,
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
    let second_region = second_delta.0.mul_add(-edge.0, second_delta.1 * -edge.1);

    // This is b2CollidePolygonAndCircle's face/vertex region selection, not
    // a generic SAT support point. The latter chooses an arbitrary extreme
    // polygon vertex along the face normal and produces the wrong lever arm
    // for an edge contact (large enough to destabilize closed joint loops).
    // The native epsilon branch classifies centers within the polygon as a
    // face contact before testing either vertex Voronoi region.
    let (polygon_to_circle, separation, plane_point) = if face_separation < f32::EPSILON {
        (face_normal, face_separation, face_center)
    } else if first_region <= 0.0_f32 {
        let distance_squared = first_delta
            .0
            .mul_add(first_delta.0, first_delta.1 * first_delta.1);
        if distance_squared > total_radius * total_radius {
            return None;
        }
        let distance = distance_squared.sqrt();
        let normal = if distance >= f32::EPSILON {
            (first_delta.0 / distance, first_delta.1 / distance)
        } else {
            // sub_10085E624 intentionally leaves the zero/small delta in the
            // manifold instead of falling back to the adjacent face normal.
            first_delta
        };
        (normal, distance, first_vertex)
    } else if second_region <= 0.0_f32 {
        let distance_squared = second_delta
            .0
            .mul_add(second_delta.0, second_delta.1 * second_delta.1);
        if distance_squared > total_radius * total_radius {
            return None;
        }
        let distance = distance_squared.sqrt();
        let normal = if distance >= f32::EPSILON {
            (second_delta.0 / distance, second_delta.1 / distance)
        } else {
            second_delta
        };
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

    // Reconstruct b2WorldManifold's face-A points in native float32 order.
    let polygon_surface = (
        (polygon_radius - separation).mul_add(polygon_to_circle.0, circle_center.0),
        (polygon_radius - separation).mul_add(polygon_to_circle.1, circle_center.1),
    );
    let circle_surface = (
        (-circle_radius).mul_add(polygon_to_circle.0, circle_center.0),
        (-circle_radius).mul_add(polygon_to_circle.1, circle_center.1),
    );
    let normal = if circle_is_first {
        (-polygon_to_circle.0, -polygon_to_circle.1)
    } else {
        polygon_to_circle
    };
    Some(ContactManifold {
        normal_x: f64::from(normal.0),
        normal_y: f64::from(normal.1),
        penetration: f64::from(total_radius - separation),
        point_x: f64::from((polygon_surface.0 + circle_surface.0) * 0.5_f32),
        point_y: f64::from((polygon_surface.1 + circle_surface.1) * 0.5_f32),
        // Every branch of sub_10085E624 clears b2ManifoldPoint::id.key.
        feature_id: 0,
        secondary: None,
        position: ContactPositionState::World(if circle_is_first {
            ContactPositionWitness::FaceSecond {
                normal: polygon_to_circle,
                plane_point,
                clip_points: [circle_center, (0.0, 0.0)],
                point_count: 1,
                first_radius: circle_radius,
                second_radius: polygon_radius,
            }
        } else {
            ContactPositionWitness::FaceFirst {
                normal: polygon_to_circle,
                plane_point,
                clip_points: [circle_center, (0.0, 0.0)],
                point_count: 1,
                first_radius: polygon_radius,
                second_radius: circle_radius,
            }
        }),
    })
}
