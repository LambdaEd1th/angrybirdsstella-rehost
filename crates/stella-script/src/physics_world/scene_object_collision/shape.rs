//! Shape-local vertices and native b2Transform projection.

use crate::*;

impl SceneObject {
    /// The live `b2Transform` stored on the body.  TOI computes collision
    /// manifolds from this compact transform plus fixture-local geometry; it
    /// does not need a temporary copy of the surrounding RenderObjectData.
    pub(crate) fn native_collision_transform(&self) -> NativeToiTransform {
        let (sine, cosine) = (self.angle as f32).sin_cos();
        NativeToiTransform {
            position: (self.x as f32, self.y as f32),
            sine,
            cosine,
        }
    }

    /// Reconstruct `b2Transform` from one interpolated `b2Sweep` pose. Purple's
    /// SolveTOI advances the sweep centre/angle and derives only these four
    /// transform scalars before asking the existing fixtures for a manifold.
    pub(crate) fn native_collision_transform_at_sweep(
        &self,
        center: (f32, f32),
        angle: f32,
    ) -> NativeToiTransform {
        let local_center = self.local_center();
        let local_x = local_center.0 as f32;
        let local_y = local_center.1 as f32;
        let (sine, cosine) = angle.sin_cos();
        NativeToiTransform {
            position: (
                center.0 - local_x.mul_add(cosine, -(local_y * sine)),
                center.1 - local_x.mul_add(sine, local_y * cosine),
            ),
            sine,
            cosine,
        }
    }

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

    pub(crate) fn transform_collision_point(&self, point: (f64, f64)) -> (f64, f64) {
        self.transform_collision_point_at(self.native_collision_transform(), point)
    }

    pub(super) fn transform_collision_point_at(
        &self,
        transform: NativeToiTransform,
        point: (f64, f64),
    ) -> (f64, f64) {
        // b2Transform/b2Mul consume the float32 fixture vertex, transform and
        // scale state. Keep the two ARM fused operations used by Purple rather
        // than promoting world-space narrow-phase vertices to f64.
        let local_x = point.0 as f32 * self.physics_scale_x as f32;
        let local_y = point.1 as f32 * self.physics_scale_y as f32;
        let world = transform.point((local_x, local_y));
        (f64::from(world.0), f64::from(world.1))
    }

    #[cfg(test)]
    pub(crate) fn collision_vertices(&self) -> Vec<(f64, f64)> {
        self.collision_local_vertices()
            .into_iter()
            .map(|point| self.transform_collision_point(point))
            .collect()
    }

    pub(crate) fn collision_circle(&self) -> Option<((f64, f64), f64)> {
        self.collision_circle_at(self.native_collision_transform())
    }

    pub(super) fn collision_circle_at(
        &self,
        transform: NativeToiTransform,
    ) -> Option<((f64, f64), f64)> {
        let CollisionShape::Circle { radius } = &self.collision_shape else {
            return None;
        };
        let radius = (*radius as f32)
            * (self.physics_scale_x.abs() as f32).min(self.physics_scale_y.abs() as f32);
        Some((
            (
                f64::from(transform.position.0),
                f64::from(transform.position.1),
            ),
            f64::from(radius),
        ))
    }

    #[cfg(test)]
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

    #[cfg(test)]
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
            .collect()
    }
}
