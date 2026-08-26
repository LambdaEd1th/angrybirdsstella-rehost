//! Rope-joint vtable members at `0x10086983C/0x100869B30/0x100869C48`.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_rope_velocity_constraints<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
        step: f64,
    ) {
        Self::scale_joint_impulses(joint, step);
        if !joint.is_physical {
            return;
        }
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let delta = joint_anchor_delta(first, second, r_a, r_b);
        let length = delta.0.hypot(delta.1);
        if length > f64::from(0.001_f32) {
            let impulse = joint.distance_impulse;
            self.apply_joint_velocity_impulse(
                joint,
                first,
                second,
                delta.0 / length * impulse,
                delta.1 / length * impulse,
                0.0,
            );
        } else {
            // InitVelocityConstraints clears the cached impulse below 0.001.
            joint.distance_impulse = 0.0;
        }
    }

    pub(crate) fn solve_rope_joint_velocity<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
        step: f64,
    ) {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let delta = joint_anchor_delta(first, second, r_a, r_b);
        let length = delta.0.hypot(delta.1);
        if length <= f64::from(0.001_f32) {
            joint.distance_impulse = 0.0;
            return;
        }
        let axis = (delta.0 / length, delta.1 / length);
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        let cross_a = cross_2d(r_a, axis);
        let cross_b = cross_2d(r_b, axis);
        let inverse_effective_mass =
            mass_a + mass_b + inertia_a * cross_a * cross_a + inertia_b * cross_b * cross_b;
        if inverse_effective_mass <= f64::EPSILON {
            return;
        }
        let velocity_a = point_velocity(first, r_a);
        let velocity_b = point_velocity(second, r_b);
        let mut relative_speed =
            (velocity_b.0 - velocity_a.0) * axis.0 + (velocity_b.1 - velocity_a.1) * axis.1;
        let length_error = length - joint.rest_length;
        if length_error < 0.0 && step > f64::EPSILON {
            relative_speed += length_error / step;
        }
        let impulse = -relative_speed / inverse_effective_mass;
        let new_impulse = (joint.distance_impulse + impulse).min(0.0);
        let impulse_delta = new_impulse - joint.distance_impulse;
        joint.distance_impulse = new_impulse;
        self.apply_joint_velocity_impulse(
            joint,
            first,
            second,
            axis.0 * impulse_delta,
            axis.1 * impulse_delta,
            0.0,
        );
    }

    pub(crate) fn solve_rope_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let delta = joint_anchor_delta(first, second, r_a, r_b);
        let length = delta.0.hypot(delta.1);
        let (axis, normalized_length) = if length >= f64::from(f32::EPSILON) {
            ((delta.0 / length, delta.1 / length), length)
        } else {
            (delta, 0.0)
        };
        let raw_error = normalized_length - joint.rest_length;
        let error = raw_error.clamp(0.0, f64::from(0.2_f32));
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        let cross_a = cross_2d(r_a, axis);
        let cross_b = cross_2d(r_b, axis);
        let inverse_effective_mass =
            mass_a + mass_b + inertia_a * cross_a * cross_a + inertia_b * cross_b * cross_b;
        if inverse_effective_mass <= f64::EPSILON {
            return true;
        }
        let impulse = -error / inverse_effective_mass;
        self.apply_joint_position_impulse(
            joint,
            first,
            second,
            axis.0 * impulse,
            axis.1 * impulse,
            0.0,
        );
        raw_error < f64::from(0.001_f32)
    }
}
