//! Persistent local witnesses stored by `b2ContactPositionConstraint`.

use crate::SceneObject;

/// The compact b2Position array state consumed by contact constraints. The
/// native solver does not copy full render objects into every point pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionBodyState {
    pub(crate) center: (f32, f32),
    pub(crate) angle: f32,
}

impl PositionBodyState {
    pub(crate) fn capture(object: &SceneObject) -> Self {
        Self {
            center: object.native_world_center(),
            angle: object.angle as f32,
        }
    }

    pub(crate) fn transform_point(self, local_center: (f32, f32), point: (f32, f32)) -> (f32, f32) {
        let (sine, cosine) = self.angle.sin_cos();
        let position = (
            self.center.0 - local_center.0.mul_add(cosine, -(local_center.1 * sine)),
            self.center.1 - local_center.0.mul_add(sine, local_center.1 * cosine),
        );
        (
            point
                .0
                .mul_add(cosine, (-point.1).mul_add(sine, position.0)),
            point.0.mul_add(sine, point.1.mul_add(cosine, position.1)),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PositionContactConstraint {
    pub(crate) first_local_center: (f32, f32),
    pub(crate) second_local_center: (f32, f32),
    pub(crate) first_inverse_mass: f32,
    pub(crate) second_inverse_mass: f32,
    pub(crate) first_inverse_inertia: f32,
    pub(crate) second_inverse_inertia: f32,
    pub(crate) manifold: PositionContactManifold,
}

#[derive(Debug, Clone)]
pub(crate) enum PositionContactManifold {
    Circles {
        local_first: (f64, f64),
        local_second: (f64, f64),
        first_radius: f64,
        second_radius: f64,
    },
    FaceFirst {
        local_normal: (f64, f64),
        local_plane_point: (f64, f64),
        local_clip_points: Vec<(f64, f64)>,
        first_radius: f64,
        second_radius: f64,
    },
    FaceSecond {
        local_normal: (f64, f64),
        local_plane_point: (f64, f64),
        local_clip_points: Vec<(f64, f64)>,
        first_radius: f64,
        second_radius: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionWorldPoint {
    pub(crate) normal: (f32, f32),
    pub(crate) point: (f32, f32),
    pub(crate) separation: f32,
}
