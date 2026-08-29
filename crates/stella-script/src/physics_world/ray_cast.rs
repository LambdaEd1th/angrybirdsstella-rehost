//! Purple's float32 Box2D shape ray casts.

use crate::{NativeToiTransform, native_polygon_normals};

#[derive(Debug, Clone)]
pub(crate) struct RayHit {
    pub(crate) name: String,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) normal_x: f64,
    pub(crate) normal_y: f64,
    pub(crate) fraction: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeRayCastInput {
    pub(crate) start: (f32, f32),
    pub(crate) end: (f32, f32),
    pub(crate) max_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NativeRayCastOutput {
    normal: (f32, f32),
    fraction: f32,
}

impl NativeRayCastInput {
    pub(crate) fn complete(start: (f64, f64), end: (f64, f64)) -> Self {
        Self {
            start: (start.0 as f32, start.1 as f32),
            end: (end.0 as f32, end.1 as f32),
            max_fraction: 1.0_f32,
        }
    }

    fn hit(self, name: &str, output: NativeRayCastOutput) -> RayHit {
        // b2DynamicTree::RayCast (`sub_10086F834`) first rounds fraction * p2,
        // then folds (1 - fraction) * p1 into it with FMADD.
        let one_minus_fraction = 1.0_f32 - output.fraction;
        let point = (
            one_minus_fraction.mul_add(self.start.0, output.fraction * self.end.0),
            one_minus_fraction.mul_add(self.start.1, output.fraction * self.end.1),
        );
        RayHit {
            name: name.to_owned(),
            point_x: f64::from(point.0),
            point_y: f64::from(point.1),
            normal_x: f64::from(output.normal.0),
            normal_y: f64::from(output.normal.1),
            fraction: f64::from(output.fraction),
        }
    }
}

/// `b2CircleShape::RayCast` (`sub_10085D604`).
pub(crate) fn native_circle_ray_cast(
    name: &str,
    input: NativeRayCastInput,
    transform: NativeToiTransform,
    local_center: (f32, f32),
    radius: f32,
) -> Option<RayHit> {
    let center = transform.point(local_center);
    let offset = (input.start.0 - center.0, input.start.1 - center.1);
    let direction = (input.end.0 - input.start.0, input.end.1 - input.start.1);

    let offset_length_squared = offset.0.mul_add(offset.0, offset.1 * offset.1);
    // 0x10085D648 is an FNMSUB: retain the single rounding of radius² - |s|².
    let radius_delta = radius.mul_add(radius, -offset_length_squared);
    let projection = offset.0.mul_add(direction.0, offset.1 * direction.1);
    let direction_length_squared = direction.0.mul_add(direction.0, direction.1 * direction.1);
    let discriminant = projection.mul_add(projection, radius_delta * direction_length_squared);
    if discriminant < 0.0_f32 || direction_length_squared < f32::EPSILON {
        return None;
    }

    let root = projection + discriminant.sqrt();
    if root > -0.0_f32 {
        return None;
    }
    let numerator = -root;
    if !native_ordered_greater_or_equal(direction_length_squared * input.max_fraction, numerator) {
        return None;
    }
    let fraction = numerator / direction_length_squared;
    let mut normal = (
        direction.0.mul_add(fraction, offset.0),
        direction.1.mul_add(fraction, offset.1),
    );
    let normal_length = normal.0.mul_add(normal.0, normal.1 * normal.1).sqrt();
    if normal_length >= f32::EPSILON {
        let inverse_length = 1.0_f32 / normal_length;
        normal.0 *= inverse_length;
        normal.1 *= inverse_length;
    }
    Some(input.hit(name, NativeRayCastOutput { normal, fraction }))
}

/// `b2EdgeShape::RayCast` (`sub_10085D848`). Purple's build returns the
/// selected edge normal without rotating it out of shape-local coordinates.
pub(crate) fn native_edge_ray_cast(
    name: &str,
    input: NativeRayCastInput,
    transform: NativeToiTransform,
    first: (f32, f32),
    second: (f32, f32),
) -> Option<RayHit> {
    let local_start = transform.inverse_point(input.start);
    let local_end = transform.inverse_point(input.end);
    let direction = (local_end.0 - local_start.0, local_end.1 - local_start.1);
    let edge = (second.0 - first.0, second.1 - first.1);
    let mut normal = (edge.1, -edge.0);
    let edge_length_squared = edge.0.mul_add(edge.0, edge.1 * edge.1);
    let edge_length = edge_length_squared.sqrt();
    if edge_length >= f32::EPSILON {
        let inverse_length = 1.0_f32 / edge_length;
        normal.0 *= inverse_length;
        normal.1 *= inverse_length;
    }

    let denominator = direction.0.mul_add(normal.0, direction.1 * normal.1);
    if denominator == 0.0_f32 {
        return None;
    }
    let numerator =
        (first.0 - local_start.0).mul_add(normal.0, (first.1 - local_start.1) * normal.1);
    let fraction = numerator / denominator;
    if !native_ordered_greater_or_equal(fraction, 0.0_f32)
        || !native_ordered_greater_or_equal(input.max_fraction, fraction)
        || edge_length_squared == 0.0_f32
    {
        return None;
    }

    let point = (
        direction.0.mul_add(fraction, local_start.0),
        direction.1.mul_add(fraction, local_start.1),
    );
    let edge_fraction =
        (point.0 - first.0).mul_add(edge.0, (point.1 - first.1) * edge.1) / edge_length_squared;
    if !native_ordered_greater_or_equal(edge_fraction, 0.0_f32)
        || !native_ordered_greater_or_equal(1.0_f32, edge_fraction)
    {
        return None;
    }
    if numerator > 0.0_f32 {
        normal = (-normal.0, -normal.1);
    }
    Some(input.hit(name, NativeRayCastOutput { normal, fraction }))
}

/// `b2PolygonShape::RayCast` (`sub_10085DD64`) using the normals materialized
/// by `b2PolygonShape::Set` (`sub_10085DB9C`).
pub(crate) fn native_polygon_ray_cast(
    name: &str,
    input: NativeRayCastInput,
    transform: NativeToiTransform,
    vertices: &[(f32, f32)],
) -> Option<RayHit> {
    if vertices.is_empty() {
        return None;
    }
    let normals = native_polygon_normals(vertices);
    let local_start = transform.inverse_point(input.start);
    let local_end = transform.inverse_point(input.end);
    let direction = (local_end.0 - local_start.0, local_end.1 - local_start.1);
    let mut lower = 0.0_f32;
    let mut upper = input.max_fraction;
    let mut entry = None;

    for (index, (&vertex, &normal)) in vertices.iter().zip(&normals).enumerate() {
        let numerator =
            (vertex.0 - local_start.0).mul_add(normal.0, (vertex.1 - local_start.1) * normal.1);
        let denominator = direction.0.mul_add(normal.0, direction.1 * normal.1);
        if denominator == 0.0_f32 {
            if !native_ordered_greater_or_equal(numerator, 0.0_f32) {
                return None;
            }
        } else if native_arm_lt(denominator, 0.0_f32)
            && !native_ordered_greater_or_equal(numerator, lower * denominator)
        {
            entry = Some(index);
            lower = numerator / denominator;
        } else if denominator > 0.0_f32
            && !native_ordered_greater_or_equal(numerator, upper * denominator)
        {
            upper = numerator / denominator;
        }
        // The FCMP/B.LT at 0x10085DE30 rejects both an ordered inverted
        // interval and an unordered pair of bounds.
        if native_arm_lt(upper, lower) {
            return None;
        }
    }

    let index = entry?;
    let output = NativeRayCastOutput {
        normal: transform.rotate(normals[index]),
        fraction: lower,
    };
    Some(input.hit(name, output))
}

fn native_ordered_greater_or_equal(left: f32, right: f32) -> bool {
    matches!(
        left.partial_cmp(&right),
        Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
    )
}

fn native_arm_lt(left: f32, right: f32) -> bool {
    matches!(
        left.partial_cmp(&right),
        None | Some(std::cmp::Ordering::Less)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_circle_ray_uses_float32_fraction_and_weighted_world_point() {
        let input = NativeRayCastInput::complete((0.0, 0.0), (10.0, 0.0));
        let hit = native_circle_ray_cast(
            "circle",
            input,
            NativeToiTransform {
                position: (5.0, 0.0),
                sine: 0.0,
                cosine: 1.0,
            },
            (0.0, 0.0),
            1.0,
        )
        .unwrap();
        assert_eq!((hit.point_x, hit.point_y), (4.0, 0.0));
        assert_eq!((hit.normal_x, hit.normal_y), (-1.0, 0.0));
        assert_eq!(hit.fraction, f64::from(0.4_f32));
    }

    #[test]
    fn native_edge_ray_retains_the_local_normal_for_a_rotated_shape() {
        let input = NativeRayCastInput::complete((-2.0, 0.0), (2.0, 0.0));
        let hit = native_edge_ray_cast(
            "edge",
            input,
            NativeToiTransform {
                position: (0.0, 0.0),
                sine: 1.0,
                cosine: 0.0,
            },
            (-1.0, 0.0),
            (1.0, 0.0),
        )
        .unwrap();
        assert_eq!((hit.point_x, hit.point_y), (0.0, 0.0));
        assert_eq!((hit.normal_x, hit.normal_y), (-0.0, 1.0));
    }

    #[test]
    fn native_polygon_ray_rotates_its_selected_normal() {
        let input = NativeRayCastInput::complete((-2.0, 0.0), (2.0, 0.0));
        let vertices = [(-1.0, -0.5), (1.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        let hit = native_polygon_ray_cast(
            "polygon",
            input,
            NativeToiTransform {
                position: (0.0, 0.0),
                sine: 1.0,
                cosine: 0.0,
            },
            &vertices,
        )
        .unwrap();
        assert_eq!((hit.point_x, hit.point_y), (-0.5, 0.0));
        assert_eq!((hit.normal_x, hit.normal_y), (-1.0, 0.0));
    }

    #[test]
    fn native_polygon_ray_rejects_unordered_clip_bounds() {
        let input = NativeRayCastInput {
            start: (-2.0, 0.0),
            end: (2.0, 0.0),
            max_fraction: f32::NAN,
        };
        let vertices = [(-1.0, -0.5), (1.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        assert!(
            native_polygon_ray_cast("polygon", input, NativeToiTransform::IDENTITY, &vertices)
                .is_none()
        );
    }
}
