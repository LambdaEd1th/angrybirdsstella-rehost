//! Shared float32 geometry helpers used by Purple's narrow phase.

use crate::cross_2d;

pub(crate) fn polygon_signed_area_f32(polygon: &[(f64, f64)]) -> f32 {
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .fold(0.0_f32, |area, (&(x1, y1), &(x2, y2))| {
            (-(x2 as f32)).mul_add(y1 as f32, (x1 as f32).mul_add(y2 as f32, area))
        })
        * 0.5_f32
}

pub(crate) fn polygon_contains_point(polygon: &[(f64, f64)], point: (f64, f64)) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..polygon.len() {
        let first = polygon[index];
        let second = polygon[(index + 1) % polygon.len()];
        let edge = (second.0 - first.0, second.1 - first.1);
        let relative = (point.0 - first.0, point.1 - first.1);
        let cross = cross_2d(edge, relative);
        let projection = dot_2d(relative, edge);
        if cross.abs() <= 1e-9 && projection >= -1e-9 && projection <= dot_2d(edge, edge) + 1e-9 {
            return true;
        }
        let crosses = (first.1 > point.1) != (second.1 > point.1);
        if crosses
            && point.0 < first.0 + (point.1 - first.1) * (second.0 - first.0) / (second.1 - first.1)
        {
            inside = !inside;
        }
    }
    inside
}

pub(crate) fn dot_2d(first: (f64, f64), second: (f64, f64)) -> f64 {
    first.0 * second.0 + first.1 * second.1
}

pub(crate) fn polygon_centroid_f32(polygon: &[(f32, f32)]) -> Option<(f32, f32)> {
    let mut area = 0.0_f32;
    let mut center = (0.0_f32, 0.0_f32);
    for (&first, &second) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        let cross = first.0.mul_add(second.1, -(first.1 * second.0));
        area += cross;
        center.0 = (first.0 + second.0).mul_add(cross, center.0);
        center.1 = (first.1 + second.1).mul_add(cross, center.1);
    }
    if area.abs() <= f32::EPSILON {
        return None;
    }
    let scale = (3.0_f32 * area).recip();
    Some((center.0 * scale, center.1 * scale))
}

pub(crate) fn closest_point_on_segment(
    point: (f64, f64),
    segment: ((f64, f64), (f64, f64)),
) -> (f64, f64) {
    let edge = (segment.1.0 - segment.0.0, segment.1.1 - segment.0.1);
    let length_squared = edge.0 * edge.0 + edge.1 * edge.1;
    if length_squared <= f64::EPSILON {
        return segment.0;
    }
    let fraction = (((point.0 - segment.0.0) * edge.0 + (point.1 - segment.0.1) * edge.1)
        / length_squared)
        .clamp(0.0, 1.0);
    (
        segment.0.0 + edge.0 * fraction,
        segment.0.1 + edge.1 * fraction,
    )
}

pub(crate) fn closest_segment_points(
    first: ((f64, f64), (f64, f64)),
    second: ((f64, f64), (f64, f64)),
) -> ((f64, f64), (f64, f64)) {
    let first_edge = (first.1.0 - first.0.0, first.1.1 - first.0.1);
    let second_edge = (second.1.0 - second.0.0, second.1.1 - second.0.1);
    let denominator = cross_2d(first_edge, second_edge);
    if denominator.abs() > f64::EPSILON {
        let relative = (second.0.0 - first.0.0, second.0.1 - first.0.1);
        let first_fraction = cross_2d(relative, second_edge) / denominator;
        let second_fraction = cross_2d(relative, first_edge) / denominator;
        if (0.0..=1.0).contains(&first_fraction) && (0.0..=1.0).contains(&second_fraction) {
            let point = (
                first.0.0 + first_edge.0 * first_fraction,
                first.0.1 + first_edge.1 * first_fraction,
            );
            return (point, point);
        }
    }
    let candidates = [
        (first.0, closest_point_on_segment(first.0, second)),
        (first.1, closest_point_on_segment(first.1, second)),
        (closest_point_on_segment(second.0, first), second.0),
        (closest_point_on_segment(second.1, first), second.1),
    ];
    candidates
        .into_iter()
        .min_by(|first, second| {
            let first_distance = (first.1.0 - first.0.0).powi(2) + (first.1.1 - first.0.1).powi(2);
            let second_distance =
                (second.1.0 - second.0.0).powi(2) + (second.1.1 - second.0.1).powi(2);
            first_distance.total_cmp(&second_distance)
        })
        .unwrap_or((first.0, second.0))
}

pub(crate) fn polygon_segment_core_distance(
    polygon: &[(f64, f64)],
    segment: ((f64, f64), (f64, f64)),
) -> f64 {
    if polygon.len() < 3 {
        return f64::INFINITY;
    }
    if polygon_contains_point(polygon, segment.0) || polygon_contains_point(polygon, segment.1) {
        return 0.0;
    }
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .map(|edge| {
            let (first, second) = closest_segment_points(edge, segment);
            (second.0 - first.0).hypot(second.1 - first.1)
        })
        .fold(f64::INFINITY, f64::min)
}

pub(crate) fn normalized_axis_f32(axis: (f32, f32)) -> Option<(f32, f32)> {
    let length_squared = axis.0.mul_add(axis.0, axis.1 * axis.1);
    if length_squared <= f32::EPSILON * f32::EPSILON {
        return None;
    }
    let inverse_length = length_squared.sqrt().recip();
    Some((axis.0 * inverse_length, axis.1 * inverse_length))
}
