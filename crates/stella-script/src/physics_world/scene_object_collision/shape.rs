//! Shape-local vertices and native b2Transform projection.

use crate::*;

impl SceneObject {
    /// Direct float32 contour stored at RenderObjectData+0x168. This is not a
    /// fixture/world projection and deliberately excludes body transforms.
    pub(crate) fn collision_local_vertices(&self) -> Vec<(f64, f64)> {
        let vertices = match &self.collision_shape {
            CollisionShape::Box { width, height } => vec![
                (-width * 0.5, -height * 0.5),
                (width * 0.5, -height * 0.5),
                (width * 0.5, height * 0.5),
                (-width * 0.5, height * 0.5),
            ],
            CollisionShape::Polygon { vertices, .. } | CollisionShape::Line { vertices } => {
                vertices.clone()
            }
            CollisionShape::None | CollisionShape::Circle { .. } => Vec::new(),
        };
        vertices
            .into_iter()
            .map(|(x, y)| (f64::from(x as f32), f64::from(y as f32)))
            .collect()
    }

    pub(crate) fn collision_contains_world_point(&self, point: (f64, f64)) -> bool {
        if !point.0.is_finite() || !point.1.is_finite() {
            return false;
        }
        if let Some((center, radius)) = self.collision_circle() {
            return (point.0 - center.0).powi(2) + (point.1 - center.1).powi(2) <= radius * radius;
        }
        self.collision_polygons()
            .iter()
            .any(|polygon| polygon_contains_point(polygon, point))
    }

    pub(crate) fn transform_collision_point(&self, point: (f64, f64)) -> (f64, f64) {
        // b2Transform/b2Mul consume the float32 fixture vertex, transform and
        // scale state. Keep the two ARM fused operations used by Purple rather
        // than promoting world-space narrow-phase vertices to f64.
        let local_x = point.0 as f32 * self.physics_scale_x as f32;
        let local_y = point.1 as f32 * self.physics_scale_y as f32;
        let (sine, cosine) = (self.angle as f32).sin_cos();
        (
            f64::from(local_x.mul_add(cosine, (-local_y).mul_add(sine, self.x as f32))),
            f64::from(local_x.mul_add(sine, local_y.mul_add(cosine, self.y as f32))),
        )
    }

    pub(crate) fn collision_vertices(&self) -> Vec<(f64, f64)> {
        self.collision_local_vertices()
            .into_iter()
            .map(|point| self.transform_collision_point(point))
            .collect()
    }

    pub(crate) fn collision_circle(&self) -> Option<((f64, f64), f64)> {
        let CollisionShape::Circle { radius } = &self.collision_shape else {
            return None;
        };
        let radius = (radius.abs() as f32)
            * (self.physics_scale_x.abs() as f32).min(self.physics_scale_y.abs() as f32);
        Some((
            (f64::from(self.x as f32), f64::from(self.y as f32)),
            f64::from(radius),
        ))
    }

    pub(crate) fn collision_polygons(&self) -> Vec<Vec<(f64, f64)>> {
        let local_polygons = match &self.collision_shape {
            CollisionShape::Box { width, height } => vec![vec![
                (-width * 0.5, -height * 0.5),
                (width * 0.5, -height * 0.5),
                (width * 0.5, height * 0.5),
                (-width * 0.5, height * 0.5),
            ]],
            CollisionShape::Polygon { vertices, fixtures } => {
                if fixtures.is_empty() {
                    vec![vertices.clone()]
                } else {
                    fixtures.clone()
                }
            }
            CollisionShape::None | CollisionShape::Circle { .. } | CollisionShape::Line { .. } => {
                Vec::new()
            }
        };
        local_polygons
            .into_iter()
            .filter(|polygon| polygon.len() >= 3)
            .map(|polygon| {
                polygon
                    .into_iter()
                    .map(|point| self.transform_collision_point(point))
                    .collect()
            })
            .collect()
    }

    pub(crate) fn collision_segments(&self) -> Vec<((f64, f64), (f64, f64))> {
        let CollisionShape::Line { vertices } = &self.collision_shape else {
            return Vec::new();
        };
        vertices
            .windows(2)
            .map(|edge| {
                (
                    self.transform_collision_point(edge[0]),
                    self.transform_collision_point(edge[1]),
                )
            })
            .filter(|(start, end)| (end.0 - start.0).hypot(end.1 - start.1) > f64::EPSILON)
            .collect()
    }
}
