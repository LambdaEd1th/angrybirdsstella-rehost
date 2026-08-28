//! `b2PrismaticJoint::SolveVelocityConstraints` at `0x1008678E0`.

use super::{cached_prismatic_geometry, native_solve_2x2, native_solve_3x3};
use crate::*;

impl RenderBridge {
    pub(crate) fn solve_prismatic_joint_velocity<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
        step: f64,
    ) {
        let geometry = cached_prismatic_geometry(joint);
        let mass_a = joint.prismatic_inverse_mass_first as f32;
        let mass_b = joint.prismatic_inverse_mass_second as f32;
        let inertia_a = joint.prismatic_inverse_inertia_first as f32;
        let inertia_b = joint.prismatic_inverse_inertia_second as f32;
        let axis = (geometry.axis.0 as f32, geometry.axis.1 as f32);
        let perpendicular = (
            geometry.perpendicular.0 as f32,
            geometry.perpendicular.1 as f32,
        );
        let (s1, s2, a1, a2) = (
            geometry.s1 as f32,
            geometry.s2 as f32,
            geometry.a1 as f32,
            geometry.a2 as f32,
        );
        let matrix = joint.prismatic_mass_matrix;
        let (k11, k12, k13, k22, k23, k33) = (
            matrix.0 as f32,
            matrix.1 as f32,
            matrix.2 as f32,
            matrix.3 as f32,
            matrix.4 as f32,
            matrix.5 as f32,
        );

        let first_velocity = first.velocity();
        let second_velocity = second.velocity();
        let mut velocity_a = (first_velocity.0 as f32, first_velocity.1 as f32);
        let mut velocity_b = (second_velocity.0 as f32, second_velocity.1 as f32);
        let mut angular_a = first.angular_velocity() as f32;
        let mut angular_b = second.angular_velocity() as f32;
        let mut motor_delta = 0.0_f32;
        let old_motor_impulse = joint.motor_impulse as f32;
        let mut new_motor_impulse = old_motor_impulse;
        if joint.motor_enabled && joint.limit_state != JointLimitState::Equal {
            let velocity_delta = (velocity_b.0 - velocity_a.0, velocity_b.1 - velocity_a.1);
            let relative_speed = (-angular_a).mul_add(
                a1,
                a2.mul_add(
                    angular_b,
                    velocity_delta.0.mul_add(axis.0, velocity_delta.1 * axis.1),
                ),
            );
            let motor_mass = joint.prismatic_motor_mass as f32;
            let requested_speed = joint.motor_speed.unwrap_or(0.0) as f32 - relative_speed;
            let maximum_impulse = step as f32 * joint.max_torque as f32;
            new_motor_impulse = motor_mass
                .mul_add(requested_speed, old_motor_impulse)
                .min(maximum_impulse)
                .max(-maximum_impulse);
            motor_delta = new_motor_impulse - old_motor_impulse;
            let motor_impulse = (axis.0 * motor_delta, axis.1 * motor_delta);
            velocity_a.0 = (-mass_a).mul_add(motor_impulse.0, velocity_a.0);
            velocity_a.1 = (-mass_a).mul_add(motor_impulse.1, velocity_a.1);
            angular_a = (-inertia_a).mul_add(a1 * motor_delta, angular_a);
            velocity_b.0 = mass_b.mul_add(motor_impulse.0, velocity_b.0);
            velocity_b.1 = mass_b.mul_add(motor_impulse.1, velocity_b.1);
            angular_b = inertia_b.mul_add(a2 * motor_delta, angular_b);
        }

        let velocity_delta = (velocity_b.0 - velocity_a.0, velocity_b.1 - velocity_a.1);
        let c_dot_perpendicular = (-angular_a).mul_add(
            s1,
            s2.mul_add(
                angular_b,
                velocity_delta
                    .0
                    .mul_add(perpendicular.0, velocity_delta.1 * perpendicular.1),
            ),
        );
        let c_dot_angular = angular_b - angular_a;
        let old_perpendicular = joint.linear_impulse_x as f32;
        let old_angular = joint.angular_impulse as f32;
        let old_limit = joint.limit_impulse as f32;
        let mut perpendicular_delta: f32;
        let mut angular_delta: f32;
        let mut limit_delta = 0.0_f32;
        let mut new_limit_impulse = old_limit;

        if joint.limit_state != JointLimitState::Inactive {
            let c_dot_axis = (-angular_a).mul_add(
                a1,
                a2.mul_add(
                    angular_b,
                    velocity_delta.0.mul_add(axis.0, velocity_delta.1 * axis.1),
                ),
            );
            let delta = native_solve_3x3(
                (k11, k12, k13, k22, k23, k33),
                (-c_dot_perpendicular, -c_dot_angular, -c_dot_axis),
            );
            perpendicular_delta = delta.0;
            angular_delta = delta.1;
            limit_delta = delta.2;
            let candidate_limit = old_limit + limit_delta;
            let violates_complementarity = (joint.limit_state == JointLimitState::AtLower
                && candidate_limit < 0.0)
                || (joint.limit_state == JointLimitState::AtUpper && candidate_limit > 0.0);
            if violates_complementarity {
                limit_delta = -old_limit;
                let rhs_perpendicular = -c_dot_perpendicular - k13 * limit_delta;
                let rhs_angular = -c_dot_angular - k23 * limit_delta;
                (perpendicular_delta, angular_delta) =
                    native_solve_2x2(k11, k12, k22, rhs_perpendicular, rhs_angular);
                new_limit_impulse = 0.0;
            } else {
                new_limit_impulse = candidate_limit;
            }
        } else {
            (perpendicular_delta, angular_delta) =
                native_solve_2x2(k11, k12, k22, -c_dot_perpendicular, -c_dot_angular);
        }

        joint.linear_impulse_x = f64::from(old_perpendicular + perpendicular_delta);
        joint.angular_impulse = f64::from(old_angular + angular_delta);
        joint.limit_impulse = f64::from(new_limit_impulse);
        joint.motor_impulse = f64::from(new_motor_impulse);
        self.apply_prismatic_velocity_impulse(
            joint,
            geometry,
            (
                perpendicular_delta,
                motor_delta + limit_delta,
                angular_delta,
            ),
        );
    }
}
