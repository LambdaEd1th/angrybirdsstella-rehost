//! Distance-joint velocity and position constraints.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_distance_velocity_constraints<
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
        if length > f64::EPSILON {
            let impulse = joint.distance_impulse;
            self.apply_joint_velocity_impulse(
                joint,
                first,
                second,
                delta.0 / length * impulse,
                delta.1 / length * impulse,
                0.0,
            );
        }
    }

    pub(crate) fn solve_distance_joint_velocity<
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
        // b2DistanceJoint::InitVelocityConstraints uses a deliberately wider
        // 0.001 threshold than b2Vec2::Normalize. Short distance axes are
        // written as exactly zero while the scalar mass is still initialized.
        let axis = if length > f64::from(0.001_f32) {
            (delta.0 / length, delta.1 / length)
        } else {
            (0.0, 0.0)
        };
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
        let mut effective_mass = inverse_effective_mass.recip();
        let mut gamma = 0.0;
        let mut bias = 0.0;
        if joint.frequency > 0.0 {
            let step = f64::from(step as f32);
            let error = length - joint.rest_length;
            // The bundled Box2D build materializes 6.2832f rather than the
            // full double-precision τ constant.
            #[allow(clippy::approx_constant)]
            const NATIVE_TWO_PI: f32 = 6.2832_f32;
            let omega = f64::from(NATIVE_TWO_PI) * f64::from(joint.frequency as f32);
            let damping = 2.0 * effective_mass * joint.damping_ratio * omega;
            let stiffness = effective_mass * omega * omega;
            gamma = step * (damping + step * stiffness);
            if gamma > f64::EPSILON {
                gamma = gamma.recip();
            }
            bias = error * step * stiffness * gamma;
            effective_mass = (inverse_effective_mass + gamma).recip();
        }
        let velocity_a = point_velocity(first, r_a);
        let velocity_b = point_velocity(second, r_b);
        let relative_speed =
            (velocity_b.0 - velocity_a.0) * axis.0 + (velocity_b.1 - velocity_a.1) * axis.1;
        let impulse = -effective_mass * (relative_speed + bias + gamma * joint.distance_impulse);
        joint.distance_impulse += impulse;
        self.apply_joint_velocity_impulse(
            joint,
            first,
            second,
            axis.0 * impulse,
            axis.1 * impulse,
            0.0,
        );
    }

    pub(crate) fn solve_distance_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        if joint.frequency > 0.0 {
            return true;
        }
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let delta = joint_anchor_delta(first, second, r_a, r_b);
        let length = delta.0.hypot(delta.1);
        // b2Vec2::Normalize returns zero below FLT_EPSILON without changing
        // the vector. Preserve that tiny unnormalized direction: returning
        // early or normalizing it to unit length both produce a much larger
        // correction than the native solver.
        let (axis, normalized_length) = if length >= f64::from(f32::EPSILON) {
            ((delta.0 / length, delta.1 / length), length)
        } else {
            (delta, 0.0)
        };
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
        let raw_error = normalized_length - joint.rest_length;
        let error = raw_error.clamp(-f64::from(0.2_f32), f64::from(0.2_f32));
        let impulse = -error / inverse_effective_mass;
        self.apply_joint_position_impulse(
            joint,
            first,
            second,
            axis.0 * impulse,
            axis.1 * impulse,
            0.0,
        );
        error.abs() < f64::from(0.001_f32)
    }
}
