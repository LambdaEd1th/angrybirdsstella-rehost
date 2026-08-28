//! Per-shape ray-cast result generation used by GameLua and extensions.

use crate::*;

impl SceneObject {
    pub(crate) fn ray_cast_hits(
        &self,
        name: &str,
        start: (f64, f64),
        end: (f64, f64),
    ) -> Vec<RayHit> {
        let input = NativeRayCastInput::complete(start, end);
        let transform = self.native_collision_transform();
        match &self.collision_shape {
            CollisionShape::None => Vec::new(),
            CollisionShape::Circle { .. } => self
                .native_distance_proxy(0)
                .and_then(|proxy| {
                    native_circle_ray_cast(name, input, transform, proxy.vertices[0], proxy.radius)
                })
                .into_iter()
                .collect(),
            CollisionShape::Box { .. } | CollisionShape::Polygon { .. } => {
                (0..self.collision_shape.fixture_count())
                    .filter_map(|fixture| self.native_distance_proxy(fixture))
                    .filter_map(|proxy| {
                        native_polygon_ray_cast(name, input, transform, &proxy.vertices)
                    })
                    .collect()
            }
            CollisionShape::Line { .. } => (0..self.collision_shape.fixture_count())
                .filter_map(|fixture| self.native_distance_proxy(fixture))
                .filter_map(|proxy| {
                    native_edge_ray_cast(
                        name,
                        input,
                        transform,
                        proxy.vertices[0],
                        proxy.vertices[1],
                    )
                })
                .collect(),
        }
    }
}
