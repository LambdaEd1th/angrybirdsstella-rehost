//! `b2FindMaxSeparation`/`b2EdgeSeparation` (`0x10085FB84`/`0x10085FD74`).

use super::super::{
    NativePolygon,
    geometry::{native_arm_lt_f32, native_fmax_f32, native_fmin_f32},
};
use crate::{NativeToiTransform, native_polygon_centroid_f32, native_polygon_normals};

#[cfg(test)]
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
        .collect::<NativePolygon<_>>();
    let incident_points = incident
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    let reference_centroid = native_polygon_centroid_f32(&reference_points);
    let incident_centroid = native_polygon_centroid_f32(&incident_points);
    let centroid_delta = (
        incident_centroid.0 - reference_centroid.0,
        incident_centroid.1 - reference_centroid.1,
    );

    // b2FindMaxSeparation first picks the normal most aligned with the centroid
    // direction. It then compares the two neighbours and climbs strictly in
    // just one direction. This tie/order behavior is observable in feature IDs
    // and is not equivalent to scanning every separation.
    let mut current_index = 0;
    let mut best_alignment = -f32::MAX;
    for (index, normal) in normals.iter().copied().enumerate() {
        let alignment = normal
            .0
            .mul_add(centroid_delta.0, normal.1 * centroid_delta.1);
        let replaces_index = alignment > best_alignment;
        best_alignment = native_fmax_f32(alignment, best_alignment);
        if replaces_index {
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
            if candidate.partial_cmp(&current) != Some(std::cmp::Ordering::Greater) {
                break;
            }
            current = candidate;
            current_index = previous_index;
            previous_index = current_index.checked_sub(1).unwrap_or(reference.len() - 1);
        }
    } else {
        while next_index != current_index {
            let candidate = edge_separation(reference, incident, &normals, next_index);
            if candidate.partial_cmp(&current) != Some(std::cmp::Ordering::Greater) {
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

pub(super) fn polygon_max_separation_at_transforms(
    reference: &[(f32, f32)],
    reference_transform: NativeToiTransform,
    incident: &[(f32, f32)],
    incident_transform: NativeToiTransform,
) -> Option<(f32, usize, (f32, f32))> {
    if reference.is_empty() || incident.is_empty() {
        return None;
    }
    let normals = polygon_normals_f32(reference)?;
    let reference_centroid = native_polygon_centroid_f32(reference);
    let incident_centroid = native_polygon_centroid_f32(incident);
    let reference_world_centroid = reference_transform.point(reference_centroid);
    let incident_world_centroid = incident_transform.point(incident_centroid);
    let centroid_delta = reference_transform.inverse_rotate((
        incident_world_centroid.0 - reference_world_centroid.0,
        incident_world_centroid.1 - reference_world_centroid.1,
    ));

    // b2FindMaxSeparation chooses the centroid-facing normal first, then walks only
    // one neighbouring direction. Keep its strict comparisons: equal values
    // retain the lower/earlier feature selected by the native loop.
    let mut current_index = 0;
    let mut best_alignment = -f32::MAX;
    for (index, normal) in normals.iter().copied().enumerate() {
        let alignment = normal
            .0
            .mul_add(centroid_delta.0, normal.1 * centroid_delta.1);
        let replaces_index = alignment > best_alignment;
        best_alignment = native_fmax_f32(alignment, best_alignment);
        if replaces_index {
            current_index = index;
        }
    }

    let separation = |edge| {
        polygon_edge_separation_at_transforms(
            reference,
            &normals,
            reference_transform,
            edge,
            incident,
            incident_transform,
        )
    };
    let mut current = separation(current_index);
    let mut previous_index = current_index.checked_sub(1).unwrap_or(reference.len() - 1);
    let previous = separation(previous_index);
    let mut next_index = if current_index + 1 < reference.len() {
        current_index + 1
    } else {
        0
    };
    let next = separation(next_index);

    if previous > current && previous > next {
        while previous_index != current_index {
            let candidate = separation(previous_index);
            if candidate.partial_cmp(&current) != Some(std::cmp::Ordering::Greater) {
                break;
            }
            current = candidate;
            current_index = previous_index;
            previous_index = current_index.checked_sub(1).unwrap_or(reference.len() - 1);
        }
    } else {
        while next_index != current_index {
            let candidate = separation(next_index);
            if candidate.partial_cmp(&current) != Some(std::cmp::Ordering::Greater) {
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

    Some((current, current_index, normals[current_index]))
}

fn polygon_edge_separation_at_transforms(
    reference: &[(f32, f32)],
    reference_normals: &[(f32, f32)],
    reference_transform: NativeToiTransform,
    edge: usize,
    incident: &[(f32, f32)],
    incident_transform: NativeToiTransform,
) -> f32 {
    let world_normal = reference_transform.rotate(reference_normals[edge]);
    let incident_normal = incident_transform.inverse_rotate(world_normal);
    let mut support = incident[0];
    let mut support_projection = incident_normal
        .0
        .mul_add(support.0, incident_normal.1 * support.1);
    for point in incident.iter().copied().skip(1) {
        let projection = incident_normal
            .0
            .mul_add(point.0, incident_normal.1 * point.1);
        let replaces_support = native_arm_lt_f32(projection, support_projection);
        support_projection = native_fmin_f32(projection, support_projection);
        if replaces_support {
            support = point;
        }
    }
    let reference_point = reference_transform.point(reference[edge]);
    let incident_point = incident_transform.point(support);
    world_normal.0.mul_add(
        incident_point.0 - reference_point.0,
        world_normal.1 * (incident_point.1 - reference_point.1),
    )
}

#[cfg(test)]
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
        let replaces_support = native_arm_lt_f32(projection, support_projection);
        support_projection = native_fmin_f32(projection, support_projection);
        if replaces_support {
            support = point;
        }
    }
    let reference_point = (reference[edge].0 as f32, reference[edge].1 as f32);
    let delta = (support.0 - reference_point.0, support.1 - reference_point.1);
    normal.0.mul_add(delta.0, normal.1 * delta.1)
}

#[cfg(test)]
fn polygon_normals(polygon: &[(f64, f64)]) -> Option<NativePolygon<(f32, f32)>> {
    let polygon = polygon
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<NativePolygon<_>>();
    Some(native_polygon_normals(&polygon).into_iter().collect())
}

pub(in crate::physics_world::narrow_phase) fn polygon_normals_f32(
    polygon: &[(f32, f32)],
) -> Option<NativePolygon<(f32, f32)>> {
    Some(native_polygon_normals(polygon).into_iter().collect())
}

type NativePoint2 = (f32, f32);
type NativeIndexedEdge = (usize, (NativePoint2, NativePoint2));

pub(super) fn polygon_incident_edge_at_transforms(
    polygon: &[(f32, f32)],
    polygon_transform: NativeToiTransform,
    reference_normal: (f32, f32),
    reference_transform: NativeToiTransform,
) -> Option<NativeIndexedEdge> {
    let normals = polygon_normals_f32(polygon)?;
    let incident_reference_normal =
        polygon_transform.inverse_rotate(reference_transform.rotate(reference_normal));
    let mut best_index = 0;
    let mut best_alignment = f32::MAX;
    for (index, normal) in normals.into_iter().enumerate() {
        let alignment = normal.0.mul_add(
            incident_reference_normal.0,
            normal.1 * incident_reference_normal.1,
        );
        let replaces_index = native_arm_lt_f32(alignment, best_alignment);
        best_alignment = native_fmin_f32(alignment, best_alignment);
        if replaces_index {
            best_index = index;
        }
    }
    Some((
        best_index,
        (
            polygon_transform.point(polygon[best_index]),
            polygon_transform.point(polygon[(best_index + 1) % polygon.len()]),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unordered_support_projection_advances_index_while_fmin_stays_nan() {
        let reference = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let normals = native_polygon_normals(&reference);
        let incident = [(0.0_f32, 10.0_f32), (f32::NAN, 0.0), (0.0, 0.0)];

        let separation = polygon_edge_separation_at_transforms(
            &reference,
            &normals,
            NativeToiTransform::IDENTITY,
            0,
            &incident,
            NativeToiTransform::IDENTITY,
        );

        assert_eq!(separation.to_bits(), (-1.0_f32).to_bits());
    }
}
