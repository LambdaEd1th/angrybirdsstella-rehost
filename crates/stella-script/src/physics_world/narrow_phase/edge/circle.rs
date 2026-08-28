//! b2CollideEdgeAndCircle (`sub_10085E8AC`).

use crate::{
    BOX2D_POLYGON_RADIUS, ContactLocalManifold, ContactManifold, ContactManifoldType,
    NativeToiTransform, contact_feature_id,
};

#[cfg(test)]
pub(crate) fn circle_segment_manifold(
    circle_center: (f64, f64),
    circle_radius: f64,
    segment: ((f64, f64), (f64, f64)),
    circle_is_first: bool,
) -> Option<ContactManifold> {
    circle_segment_manifold_at_transforms(
        (circle_center.0 as f32, circle_center.1 as f32),
        circle_radius as f32,
        NativeToiTransform::IDENTITY,
        (
            (segment.0.0 as f32, segment.0.1 as f32),
            (segment.1.0 as f32, segment.1.1 as f32),
        ),
        NativeToiTransform::IDENTITY,
        circle_is_first,
    )
}

pub(crate) fn circle_segment_manifold_at_transforms(
    circle_local_center: (f32, f32),
    circle_radius: f32,
    circle_transform: NativeToiTransform,
    segment_local: ((f32, f32), (f32, f32)),
    segment_transform: NativeToiTransform,
    circle_is_first: bool,
) -> Option<ContactManifold> {
    // sub_10085E8B0..0x10085E8F4 transforms the circle shape's local centre
    // into world space and then through the inverse edge transform. All three
    // region tests that follow are edge-local.
    let circle_center =
        segment_transform.inverse_point(circle_transform.point(circle_local_center));
    let edge_radius = BOX2D_POLYGON_RADIUS as f32;
    let radius_sum = circle_radius + edge_radius;
    let start = segment_local.0;
    let end = segment_local.1;
    let edge = (end.0 - start.0, end.1 - start.1);
    let from_start = (circle_center.0 - start.0, circle_center.1 - start.1);
    let from_end = (end.0 - circle_center.0, end.1 - circle_center.1);
    let start_region = from_start.0.mul_add(edge.0, from_start.1 * edge.1);
    let end_region = from_end.0.mul_add(edge.0, from_end.1 * edge.1);

    let (closest, mut edge_to_circle, separation, segment_index, segment_type) =
        if start_region <= 0.0_f32 {
            let distance_squared = from_start
                .0
                .mul_add(from_start.0, from_start.1 * from_start.1);
            if distance_squared > radius_sum * radius_sum {
                return None;
            }
            let distance = distance_squared.sqrt();
            let normal = if distance_squared > f32::EPSILON * f32::EPSILON {
                let inverse_distance = distance.recip();
                (
                    from_start.0 * inverse_distance,
                    from_start.1 * inverse_distance,
                )
            } else {
                (1.0_f32, 0.0_f32)
            };
            (start, normal, distance, 0, 0)
        } else if end_region <= 0.0_f32 {
            let circle_from_end = (-from_end.0, -from_end.1);
            let distance_squared = circle_from_end
                .0
                .mul_add(circle_from_end.0, circle_from_end.1 * circle_from_end.1);
            if distance_squared > radius_sum * radius_sum {
                return None;
            }
            let distance = distance_squared.sqrt();
            let normal = if distance_squared > f32::EPSILON * f32::EPSILON {
                let inverse_distance = distance.recip();
                (
                    circle_from_end.0 * inverse_distance,
                    circle_from_end.1 * inverse_distance,
                )
            } else {
                (1.0_f32, 0.0_f32)
            };
            (end, normal, distance, 1, 0)
        } else {
            let denominator = edge.0.mul_add(edge.0, edge.1 * edge.1);
            if denominator <= f32::EPSILON * f32::EPSILON {
                return None;
            }
            // The native leaf does not materialize the closest point. It
            // rounds B*v, folds A*u into its negation with FNMADD, then forms
            // Q - (A*u + B*v) / dot(e,e) with FMADD. This is observable at
            // the exact combined-radius boundary.
            let inverse_denominator = denominator.recip();
            let weighted_x = (-start.0).mul_add(end_region, -(end.0 * start_region));
            let weighted_y = (-start.1).mul_add(end_region, -(end.1 * start_region));
            let delta = (
                weighted_x.mul_add(inverse_denominator, circle_center.0),
                weighted_y.mul_add(inverse_denominator, circle_center.1),
            );
            let distance_squared = delta.0.mul_add(delta.0, delta.1 * delta.1);
            if distance_squared > radius_sum * radius_sum {
                return None;
            }
            let side = edge.0.mul_add(from_start.1, -(from_start.0 * edge.1));
            let mut normal = if side < 0.0_f32 {
                (edge.1, -edge.0)
            } else {
                (-edge.1, edge.0)
            };
            let normal_length = normal.0.mul_add(normal.0, normal.1 * normal.1).sqrt();
            if normal_length >= f32::EPSILON {
                let inverse_length = normal_length.recip();
                normal = (normal.0 * inverse_length, normal.1 * inverse_length);
            }
            // b2WorldManifold's face branch derives separation from the edge
            // plane rather than reusing sqrt(distanceSquared).
            let separation = from_start.0.mul_add(normal.0, from_start.1 * normal.1);
            (
                (circle_center.0 - delta.0, circle_center.1 - delta.1),
                normal,
                separation,
                0,
                1,
            )
        };

    let edge_surface = if segment_type == 1 {
        // Face-A b2WorldManifold: project the circle center back onto the edge
        // plane, then advance by the edge skin radius.
        (
            (edge_radius - separation).mul_add(edge_to_circle.0, circle_center.0),
            (edge_radius - separation).mul_add(edge_to_circle.1, circle_center.1),
        )
    } else {
        (
            edge_radius.mul_add(edge_to_circle.0, closest.0),
            edge_radius.mul_add(edge_to_circle.1, closest.1),
        )
    };
    let circle_surface = (
        (-circle_radius).mul_add(edge_to_circle.0, circle_center.0),
        (-circle_radius).mul_add(edge_to_circle.1, circle_center.1),
    );
    let point_edge = (
        (edge_surface.0 + circle_surface.0) * 0.5_f32,
        (edge_surface.1 + circle_surface.1) * 0.5_f32,
    );
    let point = segment_transform.point(point_edge);
    let reference_normal = edge_to_circle;
    if circle_is_first {
        edge_to_circle = (-edge_to_circle.0, -edge_to_circle.1);
    }
    edge_to_circle = segment_transform.rotate(edge_to_circle);
    let feature_id = if circle_is_first {
        contact_feature_id(0, segment_index, 0, segment_type)
    } else {
        contact_feature_id(segment_index, 0, segment_type, 0)
    };
    let position = if segment_type == 0 {
        if circle_is_first {
            ContactLocalManifold {
                manifold_type: ContactManifoldType::Circles,
                local_normal: (0.0, 0.0),
                local_point: circle_local_center,
                local_points: [closest, (0.0, 0.0)],
                point_count: 1,
                first_radius: circle_radius,
                second_radius: edge_radius,
            }
        } else {
            ContactLocalManifold {
                manifold_type: ContactManifoldType::Circles,
                local_normal: (0.0, 0.0),
                local_point: closest,
                local_points: [circle_local_center, (0.0, 0.0)],
                point_count: 1,
                first_radius: edge_radius,
                second_radius: circle_radius,
            }
        }
    } else if circle_is_first {
        ContactLocalManifold {
            manifold_type: ContactManifoldType::FaceSecond,
            local_normal: reference_normal,
            local_point: start,
            local_points: [circle_local_center, (0.0, 0.0)],
            point_count: 1,
            first_radius: circle_radius,
            second_radius: edge_radius,
        }
    } else {
        ContactLocalManifold {
            manifold_type: ContactManifoldType::FaceFirst,
            local_normal: reference_normal,
            local_point: start,
            local_points: [circle_local_center, (0.0, 0.0)],
            point_count: 1,
            first_radius: edge_radius,
            second_radius: circle_radius,
        }
    };
    Some(ContactManifold {
        normal_x: f64::from(edge_to_circle.0),
        normal_y: f64::from(edge_to_circle.1),
        penetration: f64::from(radius_sum - separation),
        point_x: f64::from(point.0),
        point_y: f64::from(point.1),
        feature_id,
        secondary: None,
        position,
    })
}
