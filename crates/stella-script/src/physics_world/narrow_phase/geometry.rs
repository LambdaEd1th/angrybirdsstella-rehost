//! Shared float32 geometry helpers used by Purple's narrow phase.

#[cfg(test)]
use crate::cross_2d;

#[cfg(test)]
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

pub(crate) fn polygon_signed_area_from_f32(polygon: &[(f32, f32)]) -> f32 {
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .fold(0.0_f32, |area, (&(x1, y1), &(x2, y2))| {
            (-x2).mul_add(y1, x1.mul_add(y2, area))
        })
        * 0.5_f32
}

#[cfg(test)]
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

#[cfg(test)]
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

pub(crate) fn normalized_axis_f32(axis: (f32, f32)) -> Option<(f32, f32)> {
    let length_squared = axis.0.mul_add(axis.0, axis.1 * axis.1);
    if length_squared <= f32::EPSILON * f32::EPSILON {
        return None;
    }
    let inverse_length = length_squared.sqrt().recip();
    Some((axis.0 * inverse_length, axis.1 * inverse_length))
}
