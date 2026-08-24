//! `b2DistanceProxy::Set` (`sub_1008602D4`, 148 bytes/12 blocks).

use crate::*;

impl SceneObject {
    pub(crate) fn native_distance_proxy(&self, fixture: usize) -> Option<NativeDistanceProxy> {
        let scale = (self.physics_scale_x as f32, self.physics_scale_y as f32);
        let scaled = |point: (f64, f64)| ((point.0 as f32) * scale.0, (point.1 as f32) * scale.1);
        match &self.collision_shape {
            CollisionShape::None => None,
            CollisionShape::Circle { radius } if fixture == 0 => Some(NativeDistanceProxy {
                vertices: vec![(0.0_f32, 0.0_f32)],
                radius: (radius.abs() as f32) * scale.0.abs().min(scale.1.abs()),
            }),
            CollisionShape::Circle { .. } => None,
            CollisionShape::Box { width, height } if fixture == 0 => {
                let half_width = *width * 0.5;
                let half_height = *height * 0.5;
                Some(NativeDistanceProxy {
                    vertices: [
                        (-half_width, -half_height),
                        (half_width, -half_height),
                        (half_width, half_height),
                        (-half_width, half_height),
                    ]
                    .into_iter()
                    .map(scaled)
                    .collect(),
                    radius: BOX2D_POLYGON_RADIUS as f32,
                })
            }
            CollisionShape::Box { .. } => None,
            CollisionShape::Polygon { vertices, fixtures } => {
                let vertices = if fixtures.is_empty() {
                    (fixture == 0).then_some(vertices)
                } else {
                    fixtures.get(fixture)
                }?;
                (!vertices.is_empty()).then(|| NativeDistanceProxy {
                    vertices: vertices.iter().copied().map(scaled).collect(),
                    radius: BOX2D_POLYGON_RADIUS as f32,
                })
            }
            CollisionShape::Line { vertices } => {
                // Set's native chain branch can wrap the final second endpoint
                // to vertex zero. GetChildCount is vertexCount-1, however, so
                // every valid caller reaches only an ordinary adjacent pair.
                let first = *vertices.get(fixture)?;
                let second = *vertices.get(fixture + 1).or_else(|| vertices.first())?;
                Some(NativeDistanceProxy {
                    vertices: vec![scaled(first), scaled(second)],
                    radius: BOX2D_POLYGON_RADIUS as f32,
                })
            }
        }
    }
}
