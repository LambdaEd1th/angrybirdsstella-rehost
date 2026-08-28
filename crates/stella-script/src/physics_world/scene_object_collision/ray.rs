//! Per-shape ray-cast result generation used by GameLua and extensions.

use crate::*;

impl SceneObject {
    #[cfg(test)]
    pub(crate) fn ray_cast_hits(
        &self,
        name: &str,
        start: (f64, f64),
        end: (f64, f64),
    ) -> Vec<RayHit> {
        let input = NativeRayCastInput::complete(start, end);
        (0..self.collision_shape.fixture_count())
            .filter_map(|fixture| self.ray_cast_fixture_hit(name, input, fixture))
            .collect()
    }

    pub(crate) fn ray_cast_fixture_hit(
        &self,
        name: &str,
        input: NativeRayCastInput,
        fixture: usize,
    ) -> Option<RayHit> {
        let transform = self.native_collision_transform();
        match &self.collision_shape {
            CollisionShape::None => None,
            CollisionShape::Circle { .. } => {
                self.native_distance_proxy(fixture).and_then(|proxy| {
                    native_circle_ray_cast(name, input, transform, proxy.vertices[0], proxy.radius)
                })
            }
            CollisionShape::Box { .. } | CollisionShape::Polygon { .. } => self
                .native_distance_proxy(fixture)
                .and_then(|proxy| native_polygon_ray_cast(name, input, transform, &proxy.vertices)),
            CollisionShape::Line { .. } => self.native_distance_proxy(fixture).and_then(|proxy| {
                native_edge_ray_cast(name, input, transform, proxy.vertices[0], proxy.vertices[1])
            }),
        }
    }
}
