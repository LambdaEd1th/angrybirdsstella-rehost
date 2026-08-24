//! `b2RevoluteJoint::SolveVelocityConstraints` at `0x100868F20`.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_revolute_joint_velocity(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        step: f64,
    ) {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let mass_a = first.inverse_mass_for_solver() as f32;
        let mass_b = second.inverse_mass_for_solver() as f32;
        let inertia_a = first.inverse_inertia() as f32;
        let inertia_b = second.inverse_inertia() as f32;
        let r_a = (r_a.0 as f32, r_a.1 as f32);
        let r_b = (r_b.0 as f32, r_b.1 as f32);
        let matrix = joint_mass_matrix(
            f64::from(mass_a),
            f64::from(mass_b),
            f64::from(inertia_a),
            f64::from(inertia_b),
            (f64::from(r_a.0), f64::from(r_a.1)),
            (f64::from(r_b.0), f64::from(r_b.1)),
        );
        let inverse_angular_mass = inertia_a + inertia_b;
        let motor_mass = if inverse_angular_mass > 0.0_f32 {
            inverse_angular_mass.recip()
        } else {
            0.0_f32
        };
        let mut velocity_a = (first.velocity_x as f32, first.velocity_y as f32);
        let mut velocity_b = (second.velocity_x as f32, second.velocity_y as f32);
        let mut angular_velocity_a = first.angular_velocity as f32;
        let mut angular_velocity_b = second.angular_velocity as f32;
        let mut new_motor_impulse = joint.motor_impulse as f32;
        if joint.motor_enabled
            && joint.limit_state != JointLimitState::Equal
            && inverse_angular_mass != 0.0_f32
        {
            let target_speed = joint.motor_speed.unwrap_or(0.0) as f32;
            let old_impulse = joint.motor_impulse as f32;
            let maximum_impulse = step as f32 * (joint.max_torque as f32).max(0.0_f32);
            let speed_error = target_speed + angular_velocity_a - angular_velocity_b;
            let new_impulse = motor_mass
                .mul_add(speed_error, old_impulse)
                .clamp(-maximum_impulse, maximum_impulse);
            let motor_delta = new_impulse - old_impulse;
            new_motor_impulse = new_impulse;
            angular_velocity_a = (-inertia_a).mul_add(motor_delta, angular_velocity_a);
            angular_velocity_b = inertia_b.mul_add(motor_delta, angular_velocity_b);
        }

        // 0x100868F20 constructs Cdot in float32 after the motor adjustment.
        let point_velocity_a = (
            (-angular_velocity_a).mul_add(r_a.1, velocity_a.0),
            angular_velocity_a.mul_add(r_a.0, velocity_a.1),
        );
        let point_velocity_b = (
            (-angular_velocity_b).mul_add(r_b.1, velocity_b.0),
            angular_velocity_b.mul_add(r_b.0, velocity_b.1),
        );
        let point_error = (
            point_velocity_b.0 - point_velocity_a.0,
            point_velocity_b.1 - point_velocity_a.1,
        );
        let (linear_delta, limit_delta, new_limit_impulse) =
            if joint.limit_state == JointLimitState::Inactive {
                let linear = solve_symmetric_2x2(
                    matrix.0,
                    matrix.1,
                    matrix.3,
                    f64::from(point_velocity_a.0 - point_velocity_b.0),
                    f64::from(point_velocity_a.1 - point_velocity_b.1),
                )
                .unwrap_or((0.0, 0.0));
                ((linear.0 as f32, linear.1 as f32), 0.0_f32, 0.0_f32)
            } else {
                let solved = solve_symmetric_3x3(
                    matrix,
                    (
                        f64::from(point_error.0),
                        f64::from(point_error.1),
                        f64::from(angular_velocity_b - angular_velocity_a),
                    ),
                )
                .unwrap_or((0.0, 0.0, 0.0));
                let full = (-(solved.0 as f32), -(solved.1 as f32), -(solved.2 as f32));
                let old_limit_impulse = joint.limit_impulse as f32;
                let candidate = old_limit_impulse + full.2;
                let violates_limit = (joint.limit_state == JointLimitState::AtLower
                    && candidate < 0.0_f32)
                    || (joint.limit_state == JointLimitState::AtUpper && candidate > 0.0_f32);
                if violates_limit {
                    let limit_delta = -old_limit_impulse;
                    let linear = solve_symmetric_2x2(
                        matrix.0,
                        matrix.1,
                        matrix.3,
                        f64::from((old_limit_impulse * matrix.2 as f32) - point_error.0),
                        f64::from((old_limit_impulse * matrix.4 as f32) - point_error.1),
                    )
                    .unwrap_or((0.0, 0.0));
                    ((linear.0 as f32, linear.1 as f32), limit_delta, 0.0_f32)
                } else {
                    ((full.0, full.1), full.2, candidate)
                }
            };
        if let Some(live_joint) = self.joints.get_mut(&joint.name) {
            live_joint.linear_impulse_x =
                f64::from(live_joint.linear_impulse_x as f32 + linear_delta.0);
            live_joint.linear_impulse_y =
                f64::from(live_joint.linear_impulse_y as f32 + linear_delta.1);
            live_joint.limit_impulse = f64::from(new_limit_impulse);
            live_joint.motor_impulse = f64::from(new_motor_impulse);
        }

        velocity_a.0 = (-mass_a).mul_add(linear_delta.0, velocity_a.0);
        velocity_a.1 = (-mass_a).mul_add(linear_delta.1, velocity_a.1);
        let cross_a = (-r_a.1).mul_add(linear_delta.0, r_a.0 * linear_delta.1);
        angular_velocity_a = (-inertia_a).mul_add(cross_a + limit_delta, angular_velocity_a);
        velocity_b.0 = mass_b.mul_add(linear_delta.0, velocity_b.0);
        velocity_b.1 = mass_b.mul_add(linear_delta.1, velocity_b.1);
        let cross_b = (-r_b.1).mul_add(linear_delta.0, r_b.0 * linear_delta.1);
        angular_velocity_b = inertia_b.mul_add(cross_b + limit_delta, angular_velocity_b);
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.velocity_x = f64::from(velocity_a.0);
            object.velocity_y = f64::from(velocity_a.1);
            object.angular_velocity = f64::from(angular_velocity_a);
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.velocity_x = f64::from(velocity_b.0);
            object.velocity_y = f64::from(velocity_b.1);
            object.angular_velocity = f64::from(angular_velocity_b);
        }
    }
}
