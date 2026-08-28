//! Per-shape `b2Shape::TestPoint` dispatch used by DirtMechanics.

use crate::*;

impl SceneObject {
    /// Walk `b2Body::m_fixtureList` head first and invoke the concrete shape's
    /// float32 `TestPoint` virtual. Fixture vectors retain creation order, so
    /// the intrusive native list is traversed in reverse.
    pub(crate) fn collision_contains_world_point(&self, point: (f64, f64)) -> bool {
        let point = (point.0 as f32, point.1 as f32);
        let transform = self.native_collision_transform();
        (0..self.collision_shape.fixture_count())
            .rev()
            .any(|fixture| self.collision_fixture_contains_point(fixture, transform, point))
    }

    fn collision_fixture_contains_point(
        &self,
        fixture: usize,
        transform: NativeToiTransform,
        point: (f32, f32),
    ) -> bool {
        let scale = (self.physics_scale_x as f32, self.physics_scale_y as f32);
        let scaled = |(x, y): (f64, f64)| ((x as f32) * scale.0, (y as f32) * scale.1);
        match &self.collision_shape {
            CollisionShape::None | CollisionShape::Line { .. } => false,
            CollisionShape::Circle { radius } if fixture == 0 => native_circle_test_point(
                transform,
                (0.0_f32, 0.0_f32),
                (*radius as f32) * scale.0.abs().min(scale.1.abs()),
                point,
            ),
            CollisionShape::Circle { .. } => false,
            CollisionShape::Box { width, height } if fixture == 0 => {
                let half_width = *width * 0.5;
                let half_height = *height * 0.5;
                native_polygon_test_point(
                    transform,
                    &[
                        scaled((-half_width, -half_height)),
                        scaled((half_width, -half_height)),
                        scaled((half_width, half_height)),
                        scaled((-half_width, half_height)),
                    ],
                    point,
                )
            }
            CollisionShape::Box { .. } => false,
            CollisionShape::Polygon { vertices, fixtures } => {
                let vertices = if fixtures.is_empty() {
                    (fixture == 0).then_some(vertices)
                } else {
                    fixtures.get(fixture)
                };
                vertices.is_some_and(|vertices| {
                    let vertices = vertices.iter().copied().map(scaled).collect::<Vec<_>>();
                    native_polygon_test_point(transform, &vertices, point)
                })
            }
        }
    }
}

/// `b2CircleShape::TestPoint` (`0x10085D5BC`). Purple's `FCMP`/`CSET LE`
/// combination accepts an unordered comparison, so a NaN distance is inside.
fn native_circle_test_point(
    transform: NativeToiTransform,
    local_center: (f32, f32),
    radius: f32,
    point: (f32, f32),
) -> bool {
    let center = transform.point(local_center);
    let delta = (point.0 - center.0, point.1 - center.1);
    // The native pair is vector FMUL followed by FADDP, not a fused dot.
    let squared_x = delta.0 * delta.0;
    let squared_y = delta.1 * delta.1;
    let distance_squared = squared_x + squared_y;
    let radius_squared = radius * radius;
    !matches!(
        distance_squared.partial_cmp(&radius_squared),
        Some(std::cmp::Ordering::Greater)
    )
}

/// `b2PolygonShape::TestPoint` (`0x10085DCE8`). Stored vertices and normals
/// are shape-local; only a strictly positive ordered plane distance rejects
/// the point. Empty and unordered plane walks consequently return true.
fn native_polygon_test_point(
    transform: NativeToiTransform,
    vertices: &[(f32, f32)],
    point: (f32, f32),
) -> bool {
    let local_point = transform.inverse_point(point);
    let normals = native_polygon_normals(vertices);
    vertices
        .iter()
        .copied()
        .zip(normals)
        .all(|(vertex, normal)| {
            let delta = (local_point.0 - vertex.0, local_point.1 - vertex.1);
            // TestPoint uses vector FMUL/FADDP rather than the scalar FMA used by
            // PolygonShape::RayCast's numerator and denominator calculations.
            let projection_x = delta.0 * normal.0;
            let projection_y = delta.1 * normal.1;
            let projection = projection_x + projection_y;
            !matches!(
                projection.partial_cmp(&0.0_f32),
                Some(std::cmp::Ordering::Greater)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_test_point_keeps_native_unordered_le_condition() {
        assert!(native_circle_test_point(
            NativeToiTransform::IDENTITY,
            (0.0, 0.0),
            -1.0,
            (f32::NAN, 0.0),
        ));
        assert!(native_circle_test_point(
            NativeToiTransform::IDENTITY,
            (0.0, 0.0),
            -1.0,
            (1.0, 0.0),
        ));
        assert!(!native_circle_test_point(
            NativeToiTransform::IDENTITY,
            (0.0, 0.0),
            -1.0,
            (1.0001, 0.0),
        ));
    }

    #[test]
    fn polygon_test_point_uses_inverse_transform_and_unordered_plane_walk() {
        let transform = NativeToiTransform {
            position: (5.0, -2.0),
            sine: 1.0,
            cosine: 0.0,
        };
        let vertices = [(-2.0, -1.0), (2.0, -1.0), (2.0, 1.0), (-2.0, 1.0)];

        assert!(native_polygon_test_point(transform, &vertices, (5.0, -2.0)));
        assert!(native_polygon_test_point(transform, &vertices, (4.0, 0.0)));
        assert!(!native_polygon_test_point(
            transform,
            &vertices,
            (3.999, 0.0)
        ));
        let tight_boundary = [(-1.0, -1.0), (0.0, -1.0), (0.0, 1.0), (-1.0, 1.0)];
        assert!(!native_polygon_test_point(
            NativeToiTransform::IDENTITY,
            &tight_boundary,
            (5.0e-10, 0.0),
        ));
        assert!(native_polygon_test_point(
            transform,
            &vertices,
            (f32::NAN, 0.0),
        ));
        assert!(native_polygon_test_point(transform, &[], (100.0, 100.0)));
    }
}
