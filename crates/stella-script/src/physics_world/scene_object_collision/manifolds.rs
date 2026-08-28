//! Fixture-pair contact-factory dispatch and manifold collection.

use crate::*;

enum CollisionFixtureGeometry {
    Circle { center: (f64, f64), radius: f64 },
    Polygon(NativePolygon<(f64, f64)>),
    Segment(((f64, f64), (f64, f64))),
}

impl SceneObject {
    pub(crate) fn collision_fixture_manifold(
        &self,
        other: &Self,
        first_fixture: usize,
        second_fixture: usize,
    ) -> Option<ContactManifold> {
        self.collision_fixture_manifold_at_transforms(
            other,
            first_fixture,
            second_fixture,
            self.native_collision_transform(),
            other.native_collision_transform(),
        )
    }

    /// Build one contact manifold from fixture-local shapes and two compact
    /// body transforms. This is the same pointer boundary used by Box2D's TOI
    /// path and avoids cloning either complete scene/render object merely to
    /// test an interpolated sweep pose.
    pub(crate) fn collision_fixture_manifold_at_transforms(
        &self,
        other: &Self,
        first_fixture: usize,
        second_fixture: usize,
        first_transform: NativeToiTransform,
        second_transform: NativeToiTransform,
    ) -> Option<ContactManifold> {
        let first = self.collision_fixture_geometry(first_fixture, first_transform)?;
        let second = other.collision_fixture_geometry(second_fixture, second_transform)?;
        let manifold = match (first, second) {
            (
                CollisionFixtureGeometry::Circle {
                    center: first_center,
                    radius: first_radius,
                },
                CollisionFixtureGeometry::Circle {
                    center: second_center,
                    radius: second_radius,
                },
            ) => circle_circle_manifold(first_center, first_radius, second_center, second_radius),
            (
                CollisionFixtureGeometry::Circle { center, radius },
                CollisionFixtureGeometry::Polygon(polygon),
            ) => circle_polygon_manifold(center, radius, &polygon, true),
            (
                CollisionFixtureGeometry::Polygon(polygon),
                CollisionFixtureGeometry::Circle { center, radius },
            ) => circle_polygon_manifold(center, radius, &polygon, false),
            (
                CollisionFixtureGeometry::Circle { center, radius },
                CollisionFixtureGeometry::Segment(segment),
            ) => circle_segment_manifold(center, radius, segment, true),
            (
                CollisionFixtureGeometry::Segment(segment),
                CollisionFixtureGeometry::Circle { center, radius },
            ) => circle_segment_manifold(center, radius, segment, false),
            (
                CollisionFixtureGeometry::Polygon(first),
                CollisionFixtureGeometry::Polygon(second),
            ) => polygon_manifold(&first, &second),
            (
                CollisionFixtureGeometry::Polygon(polygon),
                CollisionFixtureGeometry::Segment(segment),
            ) => polygon_segment_manifold(&polygon, segment, true),
            (
                CollisionFixtureGeometry::Segment(segment),
                CollisionFixtureGeometry::Polygon(polygon),
            ) => polygon_segment_manifold(&polygon, segment, false),
            (CollisionFixtureGeometry::Segment(_), CollisionFixtureGeometry::Segment(_)) => None,
        };
        manifold.map(|manifold| manifold.localize(first_transform, second_transform))
    }

    fn collision_fixture_geometry(
        &self,
        fixture: usize,
        transform: NativeToiTransform,
    ) -> Option<CollisionFixtureGeometry> {
        match &self.collision_shape {
            CollisionShape::None => None,
            CollisionShape::Circle { .. } if fixture == 0 => {
                let (center, radius) = self.collision_circle_at(transform)?;
                Some(CollisionFixtureGeometry::Circle { center, radius })
            }
            CollisionShape::Circle { .. } => None,
            CollisionShape::Box { width, height } if fixture == 0 => {
                let vertices = [
                    (-width * 0.5, -height * 0.5),
                    (width * 0.5, -height * 0.5),
                    (width * 0.5, height * 0.5),
                    (-width * 0.5, height * 0.5),
                ]
                .into_iter()
                .map(|point| self.transform_collision_point_at(transform, point))
                .collect::<NativePolygon<_>>();
                Some(CollisionFixtureGeometry::Polygon(vertices))
            }
            CollisionShape::Box { .. } => None,
            CollisionShape::Polygon { vertices, fixtures } => {
                let vertices = if fixtures.is_empty() {
                    (fixture == 0).then_some(vertices)
                } else {
                    fixtures.get(fixture)
                }?;
                (vertices.len() >= 3).then(|| {
                    CollisionFixtureGeometry::Polygon(
                        vertices
                            .iter()
                            .copied()
                            .map(|point| self.transform_collision_point_at(transform, point))
                            .collect::<NativePolygon<_>>(),
                    )
                })
            }
            CollisionShape::Line { vertices } => {
                let edge = vertices.get(fixture..fixture + 2)?;
                let start = self.transform_collision_point_at(transform, edge[0]);
                let end = self.transform_collision_point_at(transform, edge[1]);
                ((end.0 - start.0).hypot(end.1 - start.1) > f64::EPSILON)
                    .then_some(CollisionFixtureGeometry::Segment((start, end)))
            }
        }
    }

    #[cfg(test)]
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
