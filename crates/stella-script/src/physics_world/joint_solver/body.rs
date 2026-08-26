//! Compact body state consumed by one native joint-constraint pass.

use crate::SceneObject;

/// The body scalars read by Box2D joint constraints. Purple addresses these
/// through the island's compact position/velocity arrays; copying a complete
/// render object (sprites, fixtures, animation data and names) is unrelated to
/// the native solver.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JointBodyState {
    position: (f64, f64),
    center: (f32, f32),
    angle: f64,
    velocity: (f64, f64),
    angular_velocity: f64,
    inverse_mass: f64,
    inverse_inertia: f64,
    moves_during_step: bool,
    active: bool,
    motion_started: bool,
    sleeping: bool,
}

pub(crate) trait JointBodyView {
    fn native_transform_body_point(&self, point: (f32, f32)) -> (f32, f32);
    fn native_world_center(&self) -> (f32, f32);
    fn inverse_mass_for_solver(&self) -> f64;
    fn inverse_inertia(&self) -> f64;
    fn angle(&self) -> f64;
    fn velocity(&self) -> (f64, f64);
    fn angular_velocity(&self) -> f64;
}

impl JointBodyState {
    pub(crate) fn capture(object: &SceneObject) -> Self {
        Self {
            position: (object.x, object.y),
            center: object.native_world_center(),
            angle: object.angle,
            velocity: (object.velocity_x, object.velocity_y),
            angular_velocity: object.angular_velocity,
            inverse_mass: object.inverse_mass_for_solver(),
            inverse_inertia: object.inverse_inertia(),
            moves_during_step: object.moves_during_step(),
            active: object.active,
            motion_started: object.motion_started,
            sleeping: object.sleeping,
        }
    }

    pub(crate) fn participates_in_solve(self) -> bool {
        self.moves_during_step && self.active && self.motion_started && !self.sleeping
    }
}

impl JointBodyView for JointBodyState {
    fn native_transform_body_point(&self, point: (f32, f32)) -> (f32, f32) {
        let (sine, cosine) = (self.angle as f32).sin_cos();
        (
            point
                .0
                .mul_add(cosine, (-point.1).mul_add(sine, self.position.0 as f32)),
            point
                .0
                .mul_add(sine, point.1.mul_add(cosine, self.position.1 as f32)),
        )
    }

    fn native_world_center(&self) -> (f32, f32) {
        self.center
    }

    fn inverse_mass_for_solver(&self) -> f64 {
        self.inverse_mass
    }

    fn inverse_inertia(&self) -> f64 {
        self.inverse_inertia
    }

    fn angle(&self) -> f64 {
        self.angle
    }

    fn velocity(&self) -> (f64, f64) {
        self.velocity
    }

    fn angular_velocity(&self) -> f64 {
        self.angular_velocity
    }
}

impl JointBodyView for SceneObject {
    fn native_transform_body_point(&self, point: (f32, f32)) -> (f32, f32) {
        SceneObject::native_transform_body_point(self, point)
    }

    fn native_world_center(&self) -> (f32, f32) {
        SceneObject::native_world_center(self)
    }

    fn inverse_mass_for_solver(&self) -> f64 {
        SceneObject::inverse_mass_for_solver(self)
    }

    fn inverse_inertia(&self) -> f64 {
        SceneObject::inverse_inertia(self)
    }

    fn angle(&self) -> f64 {
        self.angle
    }

    fn velocity(&self) -> (f64, f64) {
        (self.velocity_x, self.velocity_y)
    }

    fn angular_velocity(&self) -> f64 {
        self.angular_velocity
    }
}
