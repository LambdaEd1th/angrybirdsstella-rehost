//! b2FindMaxSeparation and b2EdgeSeparation (`sub_10085FB84`/`sub_10085FD74`).

use super::super::geometry::{normalized_axis_f32, polygon_centroid_f32, polygon_signed_area_f32};

pub(crate) fn polygon_max_separation(
    reference: &[(f64, f64)],
    incident: &[(f64, f64)],
) -> Option<(f64, usize, (f64, f64))> {
    if reference.is_empty() || incident.is_empty() {
        return None;
    }
    let normals = polygon_normals(reference)?;
    let reference_points = reference
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<Vec<_>>();
    let incident_points = incident
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<Vec<_>>();
    let reference_centroid = polygon_centroid_f32(&reference_points)?;
    let incident_centroid = polygon_centroid_f32(&incident_points)?;
    let centroid_delta = (
        incident_centroid.0 - reference_centroid.0,
        incident_centroid.1 - reference_centroid.1,
    );

    // sub_10085FB84 first picks the normal most aligned with the centroid
    // direction. It then compares the two neighbours and climbs strictly in
    // just one direction. This tie/order behavior is observable in feature IDs
    // and is not equivalent to scanning every separation.
    let mut current_index = 0;
    let mut best_alignment = -f32::MAX;
    for (index, normal) in normals.iter().copied().enumerate() {
        let alignment = normal
            .0
            .mul_add(centroid_delta.0, normal.1 * centroid_delta.1);
        if alignment > best_alignment {
            best_alignment = alignment;
            current_index = index;
        }
    }

    let mut current = edge_separation(reference, incident, &normals, current_index);
    let mut previous_index = current_index.checked_sub(1).unwrap_or(reference.len() - 1);
    let previous = edge_separation(reference, incident, &normals, previous_index);
    let mut next_index = if current_index + 1 < reference.len() {
        current_index + 1
    } else {
        0
    };
    let next = edge_separation(reference, incident, &normals, next_index);

    if previous > current && previous > next {
        while previous_index != current_index {
            let candidate = edge_separation(reference, incident, &normals, previous_index);
            if candidate <= current {
                break;
            }
            current = candidate;
            current_index = previous_index;
            previous_index = current_index.checked_sub(1).unwrap_or(reference.len() - 1);
        }
    } else {
        while next_index != current_index {
            let candidate = edge_separation(reference, incident, &normals, next_index);
            if candidate <= current {
                break;
            }
            current = candidate;
            current_index = next_index;
            next_index = if current_index + 1 < reference.len() {
                current_index + 1
            } else {
                0
            };
        }
    }

    let normal = normals[current_index];
    Some((
        f64::from(current),
        current_index,
        (f64::from(normal.0), f64::from(normal.1)),
    ))
}

fn edge_separation(
    reference: &[(f64, f64)],
    incident: &[(f64, f64)],
    normals: &[(f32, f32)],
    edge: usize,
) -> f32 {
    let normal = normals[edge];
    let mut support = (incident[0].0 as f32, incident[0].1 as f32);
    let mut support_projection = normal.0.mul_add(support.0, normal.1 * support.1);
    for &(x, y) in &incident[1..] {
        let point = (x as f32, y as f32);
        let projection = normal.0.mul_add(point.0, normal.1 * point.1);
        if projection < support_projection {
            support_projection = projection;
            support = point;
        }
    }
    let reference_point = (reference[edge].0 as f32, reference[edge].1 as f32);
    let delta = (support.0 - reference_point.0, support.1 - reference_point.1);
    normal.0.mul_add(delta.0, normal.1 * delta.1)
}

fn polygon_normals(polygon: &[(f64, f64)]) -> Option<Vec<(f32, f32)>> {
    let orientation = polygon_signed_area_f32(polygon);
    (0..polygon.len())
        .map(|index| {
            let start = (polygon[index].0 as f32, polygon[index].1 as f32);
            let end = (
                polygon[(index + 1) % polygon.len()].0 as f32,
                polygon[(index + 1) % polygon.len()].1 as f32,
            );
            let edge = (end.0 - start.0, end.1 - start.1);
            normalized_axis_f32(if orientation >= 0.0_f32 {
                (edge.1, -edge.0)
            } else {
                (-edge.1, edge.0)
            })
        })
        .collect()
}

type CollisionPoint2 = (f64, f64);
type IndexedCollisionEdge = (usize, (CollisionPoint2, CollisionPoint2));

pub(in crate::physics_world::narrow_phase) fn polygon_incident_edge(
    polygon: &[(f64, f64)],
    reference_normal: (f64, f64),
) -> Option<IndexedCollisionEdge> {
    let normals = polygon_normals(polygon)?;
    let reference_normal = (reference_normal.0 as f32, reference_normal.1 as f32);
    let mut best = None::<(f32, usize)>;
    for (index, normal) in normals.into_iter().enumerate() {
        let alignment = normal
            .0
            .mul_add(reference_normal.0, normal.1 * reference_normal.1);
        if best.is_none_or(|candidate| alignment < candidate.0) {
            best = Some((alignment, index));
        }
    }
    best.map(|(_, index)| {
        (
            index,
            (polygon[index], polygon[(index + 1) % polygon.len()]),
        )
    })
}
