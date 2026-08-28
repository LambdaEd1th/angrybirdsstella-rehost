//! Common cached-impulse and body-impulse operations used by joint members.

use crate::*;

impl RenderBridge {
    pub(super) fn clear_constraint_impulses(
        constraints: &mut NativeIslandJointConstraints,
        step: f64,
    ) {
        for entry in &mut constraints.entries {
            let joint = &mut entry.joint;
            joint.linear_impulse_x = 0.0;
            joint.linear_impulse_y = 0.0;
            joint.angular_impulse = 0.0;
            joint.motor_impulse = 0.0;
            joint.limit_impulse = 0.0;
            joint.distance_impulse = 0.0;
            joint.previous_step = step;
        }
    }

    pub(crate) fn scale_joint_impulses(joint: &mut PhysicsJoint, step: f64) {
        let step = step as f32;
        let previous_step = joint.previous_step as f32;
        let step_ratio = if previous_step > 0.0_f32 {
            step / previous_step
        } else {
            1.0_f32
        };
        joint.previous_step = f64::from(step);
        joint.linear_impulse_x = f64::from(joint.linear_impulse_x as f32 * step_ratio);
        joint.linear_impulse_y = f64::from(joint.linear_impulse_y as f32 * step_ratio);
        joint.angular_impulse = f64::from(joint.angular_impulse as f32 * step_ratio);
        joint.motor_impulse = f64::from(joint.motor_impulse as f32 * step_ratio);
        joint.limit_impulse = f64::from(joint.limit_impulse as f32 * step_ratio);
        joint.distance_impulse = f64::from(joint.distance_impulse as f32 * step_ratio);
    }

    pub(crate) fn apply_cached_distance_velocity_impulse(
        &mut self,
        joint: &PhysicsJoint,
        radius_first: (f32, f32),
        radius_second: (f32, f32),
        impulse: (f32, f32),
    ) {
        let mass_first = joint.distance_inverse_mass_first as f32;
        let mass_second = joint.distance_inverse_mass_second as f32;
        let inertia_first = joint.distance_inverse_inertia_first as f32;
        let inertia_second = joint.distance_inverse_inertia_second as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.velocity_x =
                f64::from((-mass_first).mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y =
                f64::from((-mass_first).mul_add(impulse.1, object.velocity_y as f32));
            let cross = (-radius_first.1).mul_add(impulse.0, radius_first.0 * impulse.1);
            object.angular_velocity =
                f64::from((-inertia_first).mul_add(cross, object.angular_velocity as f32));
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.velocity_x = f64::from(mass_second.mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y = f64::from(mass_second.mul_add(impulse.1, object.velocity_y as f32));
            let cross = (-radius_second.1).mul_add(impulse.0, radius_second.0 * impulse.1);
            object.angular_velocity =
                f64::from(inertia_second.mul_add(cross, object.angular_velocity as f32));
        }
    }

    pub(crate) fn apply_cached_distance_position_impulse(
        &mut self,
        joint: &PhysicsJoint,
        radius_first: (f32, f32),
        radius_second: (f32, f32),
        impulse: (f32, f32),
    ) {
        let mass_first = joint.distance_inverse_mass_first as f32;
        let mass_second = joint.distance_inverse_mass_second as f32;
        let inertia_first = joint.distance_inverse_inertia_first as f32;
        let inertia_second = joint.distance_inverse_inertia_second as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            let cross = (-radius_first.1).mul_add(impulse.0, radius_first.0 * impulse.1);
            object.apply_native_position_impulse(-mass_first, impulse, -inertia_first, cross);
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            let cross = (-radius_second.1).mul_add(impulse.0, radius_second.0 * impulse.1);
            object.apply_native_position_impulse(mass_second, impulse, inertia_second, cross);
        }
    }
}
