//! Adjacent-triangle merge and final near-collinear cleanup.
//!
//! This follows sub_100871F08 and its eight-point convex output limit.

use super::{
    NativePoint,
    geometry::{native_cross, native_normalize, native_polygon_is_convex},
};

pub(super) fn native_merge_triangles(triangles: &[[NativePoint; 3]]) -> Vec<Vec<NativePoint>> {
    let mut used = triangles
        .iter()
        .map(|triangle| {
            triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[0] == triangle[2]
        })
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for start in 0..triangles.len() {
        if used[start] {
            continue;
        }
        let mut polygon = triangles[start].to_vec();
        used[start] = true;
        let mut candidate = 0;
        for _ in 0..triangles.len() * 2 {
            if !used[candidate]
                && let Some(merged) = native_merge_polygon_triangle(&polygon, triangles[candidate])
                && merged.len() < 9
                && native_polygon_is_convex(&merged)
            {
                polygon = merged;
                used[candidate] = true;
            }
            candidate = (candidate + 1) % triangles.len();
        }
        native_simplify_polygon(&mut polygon, 0.034_906_59_f32);
        if polygon.len() >= 3 {
            output.push(polygon);
        }
    }
    output
}

fn native_merge_polygon_triangle(
    polygon: &[NativePoint],
    triangle: [NativePoint; 3],
) -> Option<Vec<NativePoint>> {
    let mut shared = Vec::with_capacity(2);
    for (polygon_index, point) in polygon.iter().enumerate() {
        if let Some(triangle_index) = triangle.iter().position(|candidate| candidate == point) {
            shared.push((polygon_index, triangle_index));
        }
    }
    if shared.len() != 2 {
        return None;
    }
    let mut insertion_index = shared[0].0;
    if insertion_index == 0 && shared[1].0 == polygon.len() - 1 {
        insertion_index = polygon.len() - 1;
    }
    let triangle_index = (0..3).find(|index| *index != shared[0].1 && *index != shared[1].1)?;
    let mut merged = Vec::with_capacity(polygon.len() + 1);
    for (index, &point) in polygon.iter().enumerate() {
        merged.push(point);
        if index == insertion_index {
            merged.push(triangle[triangle_index]);
        }
    }
    Some(merged)
}

fn native_simplify_polygon(polygon: &mut Vec<NativePoint>, threshold: f32) {
    if polygon.len() < 4 {
        return;
    }
    let mut remove = vec![false; polygon.len()];
    let mut remaining = polygon.len();
    for index in 0..polygon.len() {
        let previous = polygon[(index + polygon.len() - 1) % polygon.len()];
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let incoming = native_normalize((current.0 - previous.0, current.1 - previous.1));
        let outgoing = native_normalize((next.0 - current.0, next.1 - current.1));
        let zero_length = current == previous || current == next;
        let nearly_collinear = native_cross(incoming, outgoing).abs() < threshold
            && incoming.0 * outgoing.0 + incoming.1 * outgoing.1 > 0.0;
        if remaining >= 4 && (zero_length || nearly_collinear) {
            remove[index] = true;
            remaining -= 1;
        }
    }
    if remaining != 0 && remaining != polygon.len() {
        *polygon = polygon
            .iter()
            .enumerate()
            .filter_map(|(index, &point)| (!remove[index]).then_some(point))
            .take(remaining)
            .collect();
    }
}
