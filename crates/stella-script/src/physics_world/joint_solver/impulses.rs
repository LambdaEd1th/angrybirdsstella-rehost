//! Common cached-impulse and body-impulse operations used by joint members.

use crate::*;

impl RenderBridge {
    pub(super) fn clear_joint_impulses(&mut self, joint_names: &[String], step: f64) {
        for name in joint_names {
            if let Some(joint) = self.joints.get_mut(name) {
                joint.linear_impulse_x = 0.0;
                joint.linear_impulse_y = 0.0;
                joint.angular_impulse = 0.0;
                joint.motor_impulse = 0.0;
                joint.limit_impulse = 0.0;
                joint.distance_impulse = 0.0;
                joint.previous_step = step;
            }
        }
    }

    pub(crate) fn scale_joint_impulses(&mut self, name: &str, step: f64) {
        let Some(joint) = self.joints.get_mut(name) else {
            return;
        };
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

    pub(crate) fn apply_joint_velocity_impulse(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        impulse_x: f64,
        impulse_y: f64,
        angular_impulse: f64,
    ) {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let mass_a = first.inverse_mass_for_solver() as f32;
        let mass_b = second.inverse_mass_for_solver() as f32;
        let inertia_a = first.inverse_inertia() as f32;
        let inertia_b = second.inverse_inertia() as f32;
        let r_a = (r_a.0 as f32, r_a.1 as f32);
        let r_b = (r_b.0 as f32, r_b.1 as f32);
        let impulse = (impulse_x as f32, impulse_y as f32);
        let angular_impulse = angular_impulse as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.velocity_x = f64::from((-mass_a).mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y = f64::from((-mass_a).mul_add(impulse.1, object.velocity_y as f32));
            let cross = (-r_a.1).mul_add(impulse.0, r_a.0 * impulse.1);
            object.angular_velocity = f64::from(
                (-inertia_a).mul_add(cross + angular_impulse, object.angular_velocity as f32),
            );
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.velocity_x = f64::from(mass_b.mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y = f64::from(mass_b.mul_add(impulse.1, object.velocity_y as f32));
            let cross = (-r_b.1).mul_add(impulse.0, r_b.0 * impulse.1);
            object.angular_velocity = f64::from(
                inertia_b.mul_add(cross + angular_impulse, object.angular_velocity as f32),
            );
        }
    }

    pub(crate) fn apply_joint_position_impulse(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        impulse_x: f64,
        impulse_y: f64,
        angular_impulse: f64,
    ) {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let mass_a = first.inverse_mass_for_solver() as f32;
        let mass_b = second.inverse_mass_for_solver() as f32;
        let inertia_a = first.inverse_inertia() as f32;
        let inertia_b = second.inverse_inertia() as f32;
        let r_a = (r_a.0 as f32, r_a.1 as f32);
        let r_b = (r_b.0 as f32, r_b.1 as f32);
        let impulse = (impulse_x as f32, impulse_y as f32);
        let angular_impulse = angular_impulse as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            let cross = (-r_a.1).mul_add(impulse.0, r_a.0 * impulse.1);
            object.apply_native_position_delta(
                -mass_a * impulse.0,
                -mass_a * impulse.1,
                -inertia_a * (cross + angular_impulse),
            );
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            let cross = (-r_b.1).mul_add(impulse.0, r_b.0 * impulse.1);
            object.apply_native_position_delta(
                mass_b * impulse.0,
                mass_b * impulse.1,
                inertia_b * (cross + angular_impulse),
            );
        }
    }
}
