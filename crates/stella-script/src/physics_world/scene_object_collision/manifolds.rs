//! Fixture-pair contact-factory dispatch and manifold collection.

use crate::*;

impl SceneObject {
    pub(crate) fn collision_fixture_manifold(
        &self,
        other: &Self,
        first_fixture: usize,
        second_fixture: usize,
    ) -> Option<ContactManifold> {
        self.collision_fixture_manifolds(other)
            .into_iter()
            .find_map(|(first_index, second_index, manifold)| {
                (first_index == first_fixture && second_index == second_fixture).then_some(manifold)
            })
    }

    pub(crate) fn collision_fixture_manifolds(
        &self,
        other: &Self,
    ) -> Vec<(usize, usize, ContactManifold)> {
        let first_circle = self.collision_circle();
        let second_circle = other.collision_circle();
        if let (Some((first_center, first_radius)), Some((second_center, second_radius))) =
            (first_circle, second_circle)
        {
            return circle_circle_manifold(
                first_center,
                first_radius,
                second_center,
                second_radius,
            )
            .map(|manifold| vec![(0, 0, manifold)])
            .unwrap_or_default();
        }

        let first_polygons = self.collision_polygons();
        let second_polygons = other.collision_polygons();
        let first_segments = self.collision_segments();
        let second_segments = other.collision_segments();
        let mut manifolds = Vec::new();
        if let Some((center, radius)) = first_circle {
            manifolds.extend(second_polygons.iter().enumerate().filter_map(
                |(second_index, polygon)| {
                    circle_polygon_manifold(center, radius, polygon, true)
                        .map(|manifold| (0, second_index, manifold))
                },
            ));
            manifolds.extend(second_segments.iter().enumerate().filter_map(
                |(second_index, &segment)| {
                    circle_segment_manifold(center, radius, segment, true)
                        .map(|manifold| (0, second_index, manifold))
                },
            ));
        } else if let Some((center, radius)) = second_circle {
            manifolds.extend(first_polygons.iter().enumerate().filter_map(
                |(first_index, polygon)| {
                    circle_polygon_manifold(center, radius, polygon, false)
                        .map(|manifold| (first_index, 0, manifold))
                },
            ));
            manifolds.extend(first_segments.iter().enumerate().filter_map(
                |(first_index, &segment)| {
                    circle_segment_manifold(center, radius, segment, false)
                        .map(|manifold| (first_index, 0, manifold))
                },
            ));
        } else {
            manifolds.extend(
                first_polygons
                    .iter()
                    .enumerate()
                    .flat_map(|(first_index, first)| {
                        second_polygons.iter().enumerate().filter_map(
                            move |(second_index, second)| {
                                polygon_manifold(first, second)
                                    .map(|manifold| (first_index, second_index, manifold))
                            },
                        )
                    }),
            );
            manifolds.extend(first_polygons.iter().enumerate().flat_map(
                |(first_index, polygon)| {
                    second_segments.iter().enumerate().filter_map(
                        move |(second_index, &segment)| {
                            polygon_segment_manifold(polygon, segment, true)
                                .map(|manifold| (first_index, second_index, manifold))
                        },
                    )
                },
            ));
            manifolds.extend(first_segments.iter().enumerate().flat_map(
                |(first_index, &segment)| {
                    second_polygons
                        .iter()
                        .enumerate()
                        .filter_map(move |(second_index, polygon)| {
                            polygon_segment_manifold(polygon, segment, false)
                                .map(|manifold| (first_index, second_index, manifold))
                        })
                },
            ));
            // The recovered contact factory has edge-circle and
            // edge-polygon entries, but deliberately no edge-edge class.
        }
        manifolds
    }
}
