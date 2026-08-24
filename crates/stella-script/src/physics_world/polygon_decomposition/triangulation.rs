//! Winding, repeated-vertex splitting and quality-ranked ear cutting.
//!
//! This isolates sub_10087007C/sub_1008710C0/sub_100871498 and the ear
//! validator at sub_100871D5C from the later convex-fixture merger.

use super::{
    NativePoint,
    geometry::{native_oriented_triangle, native_triangle_contains, native_triangle_quality},
};

pub(super) fn native_triangulate_polygon(vertices: &[(f64, f64)]) -> Vec<[NativePoint; 3]> {
    let mut contour = vertices
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<Vec<_>>();
    if contour.len() < 3 {
        return Vec::new();
    }

    let mut area = -(contour[0].0 * contour[contour.len() - 1].1
        - contour[contour.len() - 1].0 * contour[0].1);
    for index in 1..contour.len() {
        area -= contour[index].0 * contour[index - 1].1 - contour[index - 1].0 * contour[index].1;
    }
    area *= 0.5;
    if area > 0.0 {
        contour.reverse();
    }

    native_triangulate_clockwise(contour).unwrap_or_default()
}

pub(super) fn native_triangulate_clockwise(
    mut contour: Vec<NativePoint>,
) -> Option<Vec<[NativePoint; 3]>> {
    if contour.len() < 3 {
        return None;
    }
    if let Some((first, second)) = native_split_repeated_vertex(&contour) {
        let mut triangles = native_triangulate_clockwise(first)?;
        triangles.extend(native_triangulate_clockwise(second)?);
        return Some(triangles);
    }
    let mut triangles = Vec::with_capacity(contour.len() - 2);
    while contour.len() > 3 {
        let mut best_index = None;
        let mut best_quality = -10.0_f32;
        for index in 0..contour.len() {
            if !native_is_ear(&contour, index) {
                continue;
            }
            let previous = contour[(index + contour.len() - 1) % contour.len()];
            let current = contour[index];
            let next = contour[(index + 1) % contour.len()];
            let quality = native_triangle_quality(previous, current, next);
            if quality > best_quality {
                best_quality = quality;
                best_index = Some(index);
            }
        }
        let Some(index) = best_index else {
            return (!triangles.is_empty()).then_some(triangles);
        };
        let previous = contour[(index + contour.len() - 1) % contour.len()];
        let current = contour[index];
        let next = contour[(index + 1) % contour.len()];
        triangles.push(native_oriented_triangle(current, next, previous));
        contour.remove(index);
    }
    triangles.push(native_oriented_triangle(contour[1], contour[2], contour[0]));
    Some(triangles)
}

fn native_split_repeated_vertex(
    contour: &[NativePoint],
) -> Option<(Vec<NativePoint>, Vec<NativePoint>)> {
    for first in 0..contour.len().saturating_sub(1) {
        for second in first + 2..contour.len() {
            if first == 0 && second == contour.len() - 1 {
                continue;
            }
            if (contour[first].0 - contour[second].0).abs() < 0.001
                && (contour[first].1 - contour[second].1).abs() < 0.001
            {
                let first_contour = contour[first..second].to_vec();
                let second_contour = contour[second..]
                    .iter()
                    .chain(&contour[..first])
                    .copied()
                    .collect::<Vec<_>>();
                if first_contour.len() >= 3 && second_contour.len() >= 3 {
                    return Some((first_contour, second_contour));
                }
            }
        }
    }
    None
}

fn native_is_ear(contour: &[NativePoint], index: usize) -> bool {
    if contour.len() < 3 || index >= contour.len() {
        return false;
    }
    let previous_index = (index + contour.len() - 1) % contour.len();
    let next_index = (index + 1) % contour.len();
    let previous = contour[previous_index];
    let current = contour[index];
    let next = contour[next_index];
    let incoming = (current.0 - previous.0, current.1 - previous.1);
    let outgoing = (next.0 - current.0, next.1 - current.1);
    if incoming.0 * outgoing.1 - incoming.1 * outgoing.0 > 0.0 {
        return false;
    }
    let triangle = native_oriented_triangle(current, next, previous);
    !contour.iter().enumerate().any(|(candidate, &point)| {
        candidate != previous_index
            && candidate != index
            && candidate != next_index
            && native_triangle_contains(triangle, point)
    })
}
