//! Float32 geometric predicates shared by the decomposition members.

use super::NativePoint;

#[cfg(test)]
pub(super) fn polygon_area(vertices: &[(f64, f64)]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(&(x1, y1), &(x2, y2))| x1 * y2 - x2 * y1)
        .sum::<f64>()
        .abs()
        * 0.5
}

pub(super) fn native_triangle_contains(triangle: [NativePoint; 3], point: NativePoint) -> bool {
    if triangle.iter().all(|vertex| vertex.0 > point.0)
        || triangle.iter().all(|vertex| vertex.0 < point.0)
        || triangle.iter().all(|vertex| vertex.1 > point.1)
        || triangle.iter().all(|vertex| vertex.1 < point.1)
    {
        return false;
    }
    let offset = (point.0 - triangle[0].0, point.1 - triangle[0].1);
    let first = (triangle[1].0 - triangle[0].0, triangle[1].1 - triangle[0].1);
    let second = (triangle[2].0 - triangle[0].0, triangle[2].1 - triangle[0].1);
    let second_length = second.0 * second.0 + second.1 * second.1;
    let projection = first.0 * second.0 + first.1 * second.1;
    let offset_second = offset.0 * second.0 + offset.1 * second.1;
    let first_length = first.0 * first.0 + first.1 * first.1;
    let offset_first = offset.0 * first.0 + offset.1 * first.1;
    let inverse = 1.0 / -(projection * projection - first_length * second_length);
    let u = inverse * -(offset_first * projection - first_length * offset_second);
    if u < 0.0 {
        return false;
    }
    let v_numerator = -(projection * offset_second - offset_first * second_length);
    let v = inverse * v_numerator;
    v >= 0.0
        && inverse * (v_numerator - (offset_first * projection - first_length * offset_second))
            <= 1.0
}

pub(super) fn native_triangle_quality(
    previous: NativePoint,
    current: NativePoint,
    next: NativePoint,
) -> f32 {
    let first = native_normalize((next.0 - current.0, next.1 - current.1));
    let second = native_normalize((current.0 - previous.0, current.1 - previous.1));
    let third = native_normalize((previous.0 - next.0, previous.1 - next.1));
    native_cross(first, second)
        .abs()
        .min(native_cross(second, third).abs())
        .min(native_cross(first, third).abs())
}

pub(super) fn native_normalize(vector: NativePoint) -> NativePoint {
    let length = (vector.0 * vector.0 + vector.1 * vector.1).sqrt();
    if length >= f32::EPSILON {
        (vector.0 / length, vector.1 / length)
    } else {
        vector
    }
}

pub(super) fn native_cross(first: NativePoint, second: NativePoint) -> f32 {
    first.0 * second.1 - first.1 * second.0
}

pub(super) fn native_oriented_triangle(
    first: NativePoint,
    second: NativePoint,
    third: NativePoint,
) -> [NativePoint; 3] {
    if native_cross(
        (second.0 - first.0, second.1 - first.1),
        (third.0 - first.0, third.1 - first.1),
    ) <= 0.0
    {
        [first, third, second]
    } else {
        [first, second, third]
    }
}

pub(super) fn native_polygon_is_convex(polygon: &[NativePoint]) -> bool {
    let mut sign = None;
    for index in 0..polygon.len() {
        let previous = polygon[(index + polygon.len() - 1) % polygon.len()];
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_sign = ((previous.1 - current.1) * (next.0 - current.0)
            + (current.0 - previous.0) * (next.1 - current.1))
            >= 0.0;
        if sign.is_some_and(|sign| sign != current_sign) {
            return false;
        }
        sign = Some(current_sign);
    }
    true
}
