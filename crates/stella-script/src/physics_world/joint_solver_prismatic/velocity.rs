//! `b2PrismaticJoint::SolveVelocityConstraints` at `0x1008678E0`.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_prismatic_joint_velocity(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        step: f64,
    ) {
        let geometry = prismatic_geometry(joint, first, second);
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        let k11 = mass_a
            + mass_b
            + inertia_a * geometry.s1 * geometry.s1
            + inertia_b * geometry.s2 * geometry.s2;
        let k12 = inertia_a * geometry.s1 + inertia_b * geometry.s2;
        let k13 = inertia_a * geometry.s1 * geometry.a1 + inertia_b * geometry.s2 * geometry.a2;
        let k22 = inertia_a + inertia_b;
        let k23 = inertia_a * geometry.a1 + inertia_b * geometry.a2;
        let k33 = mass_a
            + mass_b
            + inertia_a * geometry.a1 * geometry.a1
            + inertia_b * geometry.a2 * geometry.a2;

        let mut velocity_a = (first.velocity_x, first.velocity_y);
        let mut velocity_b = (second.velocity_x, second.velocity_y);
        let mut angular_a = first.angular_velocity;
        let mut angular_b = second.angular_velocity;
        let mut motor_delta = 0.0;
        let mut new_motor_impulse = joint.motor_impulse;
        if joint.motor_enabled && joint.limit_state != JointLimitState::Equal && k33 > f64::EPSILON
        {
            let relative_speed = geometry.axis.0 * (velocity_b.0 - velocity_a.0)
                + geometry.axis.1 * (velocity_b.1 - velocity_a.1)
                + geometry.a2 * angular_b
                - geometry.a1 * angular_a;
            motor_delta = (joint.motor_speed.unwrap_or(0.0) - relative_speed) / k33;
            let maximum_impulse = step.max(0.0) * joint.max_torque.max(0.0);
            new_motor_impulse =
                (joint.motor_impulse + motor_delta).clamp(-maximum_impulse, maximum_impulse);
            motor_delta = new_motor_impulse - joint.motor_impulse;
            let motor_impulse = (geometry.axis.0 * motor_delta, geometry.axis.1 * motor_delta);
            velocity_a.0 -= mass_a * motor_impulse.0;
            velocity_a.1 -= mass_a * motor_impulse.1;
            angular_a -= inertia_a * geometry.a1 * motor_delta;
            velocity_b.0 += mass_b * motor_impulse.0;
            velocity_b.1 += mass_b * motor_impulse.1;
            angular_b += inertia_b * geometry.a2 * motor_delta;
        }

        let c_dot_perpendicular = geometry.perpendicular.0 * (velocity_b.0 - velocity_a.0)
            + geometry.perpendicular.1 * (velocity_b.1 - velocity_a.1)
            + geometry.s2 * angular_b
            - geometry.s1 * angular_a;
        let c_dot_angular = angular_b - angular_a;
        let old_perpendicular = joint.linear_impulse_x;
        let old_angular = joint.angular_impulse;
        let old_limit = joint.limit_impulse;
        let mut perpendicular_delta = 0.0;
        let mut angular_delta = 0.0;
        let mut limit_delta = 0.0;
        let mut new_limit_impulse = old_limit;

        if joint.limit_state != JointLimitState::Inactive {
            let c_dot_axis = geometry.axis.0 * (velocity_b.0 - velocity_a.0)
                + geometry.axis.1 * (velocity_b.1 - velocity_a.1)
                + geometry.a2 * angular_b
                - geometry.a1 * angular_a;
            if let Some(delta) = solve_symmetric_3x3(
                (k11, k12, k13, k22, k23, k33),
                (-c_dot_perpendicular, -c_dot_angular, -c_dot_axis),
            ) {
                perpendicular_delta = delta.0;
                angular_delta = delta.1;
                limit_delta = delta.2;
            }
            let candidate_limit = old_limit + limit_delta;
            let violates_complementarity = (joint.limit_state == JointLimitState::AtLower
                && candidate_limit < 0.0)
                || (joint.limit_state == JointLimitState::AtUpper && candidate_limit > 0.0);
            if violates_complementarity {
                limit_delta = -old_limit;
                let rhs_perpendicular = -c_dot_perpendicular - k13 * limit_delta;
                let rhs_angular = -c_dot_angular - k23 * limit_delta;
                (perpendicular_delta, angular_delta) =
                    solve_symmetric_2x2(k11, k12, k22, rhs_perpendicular, rhs_angular)
                        .unwrap_or((0.0, 0.0));
                new_limit_impulse = 0.0;
            } else {
                new_limit_impulse = candidate_limit;
            }
        } else {
            (perpendicular_delta, angular_delta) =
                solve_symmetric_2x2(k11, k12, k22, -c_dot_perpendicular, -c_dot_angular)
                    .unwrap_or((0.0, 0.0));
        }

        if let Some(live_joint) = self.joints.get_mut(&joint.name) {
            live_joint.linear_impulse_x = old_perpendicular + perpendicular_delta;
            live_joint.angular_impulse = old_angular + angular_delta;
            live_joint.limit_impulse = new_limit_impulse;
            live_joint.motor_impulse = new_motor_impulse;
        }
        self.apply_prismatic_velocity_impulse(
            joint,
            first,
            second,
            geometry,
            (
                perpendicular_delta,
                motor_delta + limit_delta,
                angular_delta,
            ),
        );
    }
}
