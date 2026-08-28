//! Object and per-fixture Box2D bounds.

use crate::*;

impl SceneObject {
    #[cfg(test)]
    pub(crate) fn collision_aabb(&self) -> Option<(f64, f64, f64, f64)> {
        if ![
            self.x,
            self.y,
            self.physics_scale_x,
            self.physics_scale_y,
            self.angle,
        ]
        .into_iter()
        .all(f64::is_finite)
        {
            return None;
        }
        if let Some((center, radius)) = self.collision_circle() {
            let bounds = native_circle_aabb(center, radius);
            return Some(tuple_f64(bounds));
        }
        let vertices = self.collision_vertices();
        native_fixture_aabb(&vertices, BOX2D_POLYGON_RADIUS as f32).map(tuple_f64)
    }

    pub(crate) fn collision_fixture_aabbs(&self) -> Vec<(f32, f32, f32, f32)> {
        (0..self.collision_shape.fixture_count())
            .filter_map(|fixture| self.collision_fixture_aabb(fixture))
            .collect()
    }

    /// Compute one fixture bound directly from its retained local vertices.
    /// b2Fixture::Synchronize walks the intrusive fixture/proxy array and
    /// calls ComputeAABB per child; it never materializes cloned vectors for
    /// every polygon in the body. Keeping that unit here also lets the solver
    /// update a proxy without allocating an intermediate body-wide AABB list.
    pub(crate) fn collision_fixture_aabb(&self, fixture: usize) -> Option<NativeAabb> {
        match &self.collision_shape {
            CollisionShape::None => None,
            CollisionShape::Circle { .. } if fixture == 0 => self
                .collision_circle()
                .map(|(center, radius)| native_circle_aabb(center, radius)),
            CollisionShape::Circle { .. } => None,
            CollisionShape::Box { width, height } if fixture == 0 => {
                let half_width = *width * 0.5;
                let half_height = *height * 0.5;
                native_transformed_fixture_aabb(
                    self,
                    &[
                        (-half_width, -half_height),
                        (half_width, -half_height),
                        (half_width, half_height),
                        (-half_width, half_height),
                    ],
                )
            }
            CollisionShape::Box { .. } => None,
            CollisionShape::Polygon { vertices, fixtures } => {
                let vertices = if fixtures.is_empty() {
                    (fixture == 0).then_some(vertices)
                } else {
                    fixtures.get(fixture)
                }?;
                (vertices.len() >= 3).then(|| native_transformed_fixture_aabb(self, vertices))?
            }
            CollisionShape::Line { vertices } => {
                let segment = vertices.get(fixture..fixture.checked_add(2)?)?;
                let start = self.transform_collision_point(segment[0]);
                let end = self.transform_collision_point(segment[1]);
                // createLineShape's loop at 0x100068238..0x1000682A4 calls
                // b2EdgeShape::Set and CreateFixture once per consecutive
                // pair. Its b2EdgeShape::ComputeAABB virtual at 0x10085D9A0
                // expands both endpoint extrema by m_radius, even when the
                // two vertices are equal.
                native_fixture_aabb(&[start, end], BOX2D_POLYGON_RADIUS as f32)
            }
        }
    }
}

fn native_transformed_fixture_aabb(
    object: &SceneObject,
    vertices: &[(f64, f64)],
) -> Option<NativeAabb> {
    native_fixture_aabb_iter(
        vertices
            .iter()
            .copied()
            .map(|point| object.transform_collision_point(point)),
        BOX2D_POLYGON_RADIUS as f32,
    )
}

fn native_circle_aabb(center: (f64, f64), radius: f64) -> (f32, f32, f32, f32) {
    let center = (center.0 as f32, center.1 as f32);
    let radius = radius as f32;
    (
        center.0 - radius,
        center.1 - radius,
        center.0 + radius,
        center.1 + radius,
    )
}

fn native_fixture_aabb(vertices: &[(f64, f64)], radius: f32) -> Option<(f32, f32, f32, f32)> {
    native_fixture_aabb_iter(vertices.iter().copied(), radius)
}

fn native_fixture_aabb_iter(
    mut vertices: impl Iterator<Item = (f64, f64)>,
    radius: f32,
) -> Option<NativeAabb> {
    let (first_x, first_y) = vertices.next()?;
    let (mut left, mut down, mut right, mut up) = (
        first_x as f32,
        first_y as f32,
        first_x as f32,
        first_y as f32,
    );
    for (x, y) in vertices {
        let (x, y) = (x as f32, y as f32);
        left = left.min(x);
        down = down.min(y);
        right = right.max(x);
        up = up.max(y);
    }
    Some((left - radius, down - radius, right + radius, up + radius))
}

#[cfg(test)]
fn tuple_f64(bounds: (f32, f32, f32, f32)) -> (f64, f64, f64, f64) {
    (
        f64::from(bounds.0),
        f64::from(bounds.1),
        f64::from(bounds.2),
        f64::from(bounds.3),
    )
}

#[cfg(test)]
mod tests {
    use super::native_fixture_aabb;

    #[test]
    fn fixture_skin_is_added_in_native_float32_precision() {
        let x = f32::from_bits(0x2dbc_7c70);
        let bounds = native_fixture_aabb(&[(f64::from(x), 0.0)], 0.002_f32).unwrap();

        assert_eq!(bounds.0.to_bits(), (x - 0.002_f32).to_bits());
        assert_ne!(
            bounds.0.to_bits(),
            ((f64::from(x) - 0.002_f64) as f32).to_bits()
        );
    }

    #[test]
    fn independent_edge_bounds_include_polygon_skin() {
        let bounds = native_fixture_aabb(&[(-1.25, 2.5), (3.75, -4.0)], 0.002_f32).unwrap();

        assert_eq!(
            bounds,
            (
                -1.25_f32 - 0.002_f32,
                -4.0_f32 - 0.002_f32,
                3.75_f32 + 0.002_f32,
                2.5_f32 + 0.002_f32,
            )
        );
    }

    #[test]
    fn degenerate_edge_bounds_retain_skin_and_fixture_slot() {
        let bounds = native_fixture_aabb(&[(2.0, -3.0), (2.0, -3.0)], 0.002_f32).unwrap();

        assert_eq!(
            bounds,
            (
                2.0_f32 - 0.002_f32,
                -3.0_f32 - 0.002_f32,
                2.0_f32 + 0.002_f32,
                -3.0_f32 + 0.002_f32,
            )
        );
    }
}
