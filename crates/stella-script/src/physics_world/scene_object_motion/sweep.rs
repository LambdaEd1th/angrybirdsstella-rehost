//! `b2Sweep` centre-of-mass state and transform reconstruction.

use crate::*;

impl SceneObject {
    pub(crate) fn local_center(&self) -> (f64, f64) {
        self.native_fixture_mass_data().1
    }

    pub(crate) fn world_center(&self) -> (f64, f64) {
        (
            f64::from(self.sweep_center_x),
            f64::from(self.sweep_center_y),
        )
    }

    pub(crate) fn native_world_center(&self) -> (f32, f32) {
        (self.sweep_center_x, self.sweep_center_y)
    }

    pub(crate) fn native_center_from_transform(&self) -> (f32, f32) {
        let local_center = self.local_center();
        let local_x = local_center.0 as f32;
        let local_y = local_center.1 as f32;
        let (sine, cosine) = (self.angle as f32).sin_cos();
        (
            local_x.mul_add(cosine, (-local_y).mul_add(sine, self.x as f32)),
            local_x.mul_add(sine, local_y.mul_add(cosine, self.y as f32)),
        )
    }

    pub(crate) fn sync_native_sweep_from_transform(&mut self) {
        let center = self.native_center_from_transform();
        self.sweep_center_x = center.0;
        self.sweep_center_y = center.1;
    }

    pub(crate) fn apply_native_position_delta(&mut self, center_x: f32, center_y: f32, angle: f32) {
        let new_center = (
            self.sweep_center_x + center_x,
            self.sweep_center_y + center_y,
        );
        let new_angle = self.angle as f32 + angle;
        self.set_native_sweep_transform(new_center, new_angle);
    }

    /// Integrate the transform branch used by Purple's dedicated one-body
    /// trajectory step (`sub_10086F6AC`). Unlike the island solver, that
    /// routine advances the sweep with fused multiply-adds directly from the
    /// post-damping velocities before rebuilding the body transform.
    pub(crate) fn apply_native_trajectory_velocity_step(
        &mut self,
        step: f32,
        velocity_x: f32,
        velocity_y: f32,
        angular_velocity: f32,
    ) {
        let center = (
            velocity_x.mul_add(step, self.sweep_center_x),
            velocity_y.mul_add(step, self.sweep_center_y),
        );
        let angle = angular_velocity.mul_add(step, self.angle as f32);
        self.set_native_sweep_transform(center, angle);
    }

    pub(crate) fn set_native_sweep_transform(&mut self, center: (f32, f32), angle: f32) {
        let local_center = self.local_center();
        let local_x = local_center.0 as f32;
        let local_y = local_center.1 as f32;
        let (sine, cosine) = angle.sin_cos();
        let rotated_x = local_x.mul_add(cosine, -(local_y * sine));
        let rotated_y = local_x.mul_add(sine, local_y * cosine);
        self.x = f64::from(center.0 - rotated_x);
        self.y = f64::from(center.1 - rotated_y);
        self.angle = f64::from(angle);
        self.sweep_center_x = center.0;
        self.sweep_center_y = center.1;
    }

    pub(crate) fn apply_position_delta(&mut self, center_x: f64, center_y: f64, angle: f64) {
        let old_center = self.world_center();
        // b2Sweep::a is continuous. RenderObject::setAngle normalizes its
        // explicit input, but subsequent Box2D integration and joint
        // corrections are not wrapped back into [0, 2π).
        let new_angle = self.angle + angle;
        let local_center = self.local_center();
        let rotated_center = rotate_vector(local_center, new_angle);
        self.x = old_center.0 + center_x - rotated_center.0;
        self.y = old_center.1 + center_y - rotated_center.1;
        self.angle = new_angle;
        self.sweep_center_x = (old_center.0 + center_x) as f32;
        self.sweep_center_y = (old_center.1 + center_y) as f32;
    }

    pub(crate) fn preserve_velocity_after_mass_reset(&mut self, old_center: (f64, f64)) {
        if !self.dynamic_body {
            return;
        }
        let new_center = self.native_world_center();
        let delta_x = new_center.0 - old_center.0 as f32;
        let delta_y = new_center.1 - old_center.1 as f32;
        let angular_velocity = self.angular_velocity as f32;
        // b2Body::ResetMassData keeps the transform origin fixed and adjusts
        // the COM velocity by cross(angularVelocity, newCenter-oldCenter).
        self.velocity_x = f64::from(angular_velocity.mul_add(-delta_y, self.velocity_x as f32));
        self.velocity_y = f64::from(angular_velocity.mul_add(delta_x, self.velocity_y as f32));
    }
}
