//! Persistent local witnesses stored by `b2ContactPositionConstraint`.

use crate::SceneObject;

/// The compact b2Position array state consumed by contact constraints. The
/// native solver does not copy full render objects into every point pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionBodyState {
    pub(crate) position: (f32, f32),
    pub(crate) angle: f32,
    pub(crate) center: (f32, f32),
    pub(crate) inverse_mass: f32,
    pub(crate) inverse_inertia: f32,
    pub(crate) dynamic: bool,
}

impl PositionBodyState {
    pub(crate) fn capture(object: &SceneObject) -> Self {
        Self {
            position: (object.x as f32, object.y as f32),
            angle: object.angle as f32,
            center: object.native_world_center(),
            inverse_mass: object.inverse_mass_for_solver() as f32,
            inverse_inertia: object.inverse_inertia() as f32,
            dynamic: object.dynamic_body,
        }
    }

    pub(crate) fn transform_point(self, point: (f32, f32)) -> (f32, f32) {
        let (sine, cosine) = self.angle.sin_cos();
        (
            point
                .0
                .mul_add(cosine, (-point.1).mul_add(sine, self.position.0)),
            point
                .0
                .mul_add(sine, point.1.mul_add(cosine, self.position.1)),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PositionContactConstraint {
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
