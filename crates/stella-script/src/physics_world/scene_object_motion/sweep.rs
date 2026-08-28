//! `b2Sweep` centre-of-mass state and transform reconstruction.

use crate::*;

fn native_transform_position_from_sweep(
    center: (f32, f32),
    local_center: (f32, f32),
    angle: f32,
) -> (f32, f32) {
    let (sine, cosine) = angle.sin_cos();
    let negative_rotated_x = local_center.1.mul_add(sine, -(local_center.0 * cosine));
    let rotated_y = local_center.0.mul_add(sine, local_center.1 * cosine);
    (center.0 + negative_rotated_x, center.1 - rotated_y)
}

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

    #[cfg(test)]
    pub(crate) fn apply_native_position_delta(&mut self, center_x: f32, center_y: f32, angle: f32) {
        let new_center = (
            self.sweep_center_x + center_x,
            self.sweep_center_y + center_y,
        );
        let new_angle = self.angle as f32 + angle;
        self.set_native_sweep_transform(new_center, new_angle);
    }

    pub(crate) fn apply_native_position_impulse(
        &mut self,
        inverse_mass: f32,
        impulse: (f32, f32),
        inverse_inertia: f32,
        angular_impulse: f32,
    ) {
        let new_center = (
            inverse_mass.mul_add(impulse.0, self.sweep_center_x),
            inverse_mass.mul_add(impulse.1, self.sweep_center_y),
        );
        let new_angle = inverse_inertia.mul_add(angular_impulse, self.angle as f32);
        self.set_native_sweep_transform(new_center, new_angle);
    }

    /// Integrate post-clamp velocities with the fused writes shared by
    /// Purple's ordinary island, TOI island and one-body trajectory paths.
    pub(crate) fn apply_native_velocity_step(
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
        // b2Island::Solve writes transform.p.x at 0x10086D3BC..D3D0 by
        // rounding localCenter.x*cos first, then fusing the negative rotated
        // x before the final centre FADD.
        let position = native_transform_position_from_sweep(center, (local_x, local_y), angle);
        self.x = f64::from(position.0);
        self.y = f64::from(position.1);
        self.angle = f64::from(angle);
        self.sweep_center_x = center.0;
        self.sweep_center_y = center.1;
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

#[cfg(test)]
mod tests {
    use super::native_transform_position_from_sweep;

    #[test]
    fn sweep_writeback_rounds_local_x_cos_before_native_fnmsub() {
        let center = (f32::from_bits(0x3F05_330A), f32::from_bits(0xBF02_86E1));
        let local_center = (f32::from_bits(0x3F2C_0FBB), f32::from_bits(0xBF33_41DE));
        let angle = f32::from_bits(0x3E1A_C320);
        let native = native_transform_position_from_sweep(center, local_center, angle);
        assert_eq!(native.0.to_bits(), 0xBE7F_8F1C);
        assert_eq!(native.1.to_bits(), 0x3DA6_4060);

        let (sine, cosine) = angle.sin_cos();
        let old_x = center.0 - local_center.0.mul_add(cosine, -(local_center.1 * sine));
        assert_eq!(old_x.to_bits(), 0xBE7F_8F18);
    }
}
