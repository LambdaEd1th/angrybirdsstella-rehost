//! `objectAndTrackOverlap` (`sub_10003D208`) distance wrapper.

use crate::*;

impl SceneObject {
    pub(crate) fn head_fixture_overlaps_track_segment(
        &self,
        track_segment: ((f64, f64), (f64, f64)),
    ) -> bool {
        // The native wrapper passes only b2Body::m_fixtureList to b2Distance
        // for every child edge. Creation-order vectors therefore select last.
        let distance_limit = |core_distance: f64, combined_radius: f64| {
            let epsilon = f64::from(f32::EPSILON);
            let residual = if core_distance > combined_radius && core_distance > epsilon {
                core_distance - combined_radius
            } else {
                0.0
            };
            residual < epsilon
        };
        match &self.collision_shape {
            CollisionShape::None => false,
            CollisionShape::Circle { radius } => {
                let center = (f64::from(self.x as f32), f64::from(self.y as f32));
                let closest = closest_point_on_segment(center, track_segment);
                let core_distance = (closest.0 - center.0).hypot(closest.1 - center.1);
                let native_radius = f64::from(
                    (*radius as f32)
                        * (self.physics_scale_x as f32)
                            .abs()
                            .min((self.physics_scale_y as f32).abs()),
                );
                distance_limit(core_distance, native_radius + BOX2D_POLYGON_RADIUS)
            }
            CollisionShape::Box { width, height } => {
                let polygon = [
                    (-width * 0.5, -height * 0.5),
                    (width * 0.5, -height * 0.5),
                    (width * 0.5, height * 0.5),
                    (-width * 0.5, height * 0.5),
                ]
                .into_iter()
                .map(|point| self.transform_collision_point(point))
                .collect::<Vec<_>>();
                distance_limit(
                    polygon_segment_core_distance(&polygon, track_segment),
                    2.0 * BOX2D_POLYGON_RADIUS,
                )
            }
            CollisionShape::Polygon { vertices, fixtures } => {
                let head = fixtures.last().unwrap_or(vertices);
                let polygon = head
                    .iter()
                    .copied()
                    .map(|point| self.transform_collision_point(point))
                    .collect::<Vec<_>>();
                distance_limit(
                    polygon_segment_core_distance(&polygon, track_segment),
                    2.0 * BOX2D_POLYGON_RADIUS,
                )
            }
            CollisionShape::Line { vertices } => vertices
                .windows(2)
                .next_back()
                .map(|edge| {
                    let object_segment = (
                        self.transform_collision_point(edge[0]),
                        self.transform_collision_point(edge[1]),
                    );
                    let (first, second) = closest_segment_points(object_segment, track_segment);
                    distance_limit(
                        (second.0 - first.0).hypot(second.1 - first.1),
                        2.0 * BOX2D_POLYGON_RADIUS,
                    )
                })
                .unwrap_or(false),
        }
    }
}
