//! `b2RevoluteJoint::SolvePositionConstraints` at `0x1008692B4`.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_revolute_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        const ANGULAR_SLOP: f32 = f32::from_bits(0x3d0e_fa36);
        const MAX_ANGULAR_CORRECTION: f32 = f32::from_bits(0x3e0e_fa36);
        let inertia_a = joint.revolute_inverse_inertia_first as f32;
        let inertia_b = joint.revolute_inverse_inertia_second as f32;
        let inverse_angular_mass = inertia_a + inertia_b;
        let motor_mass = joint.revolute_motor_mass as f32;
        let mut angular_error = 0.0_f32;
        if joint.limit_state != JointLimitState::Inactive && inverse_angular_mass != 0.0_f32 {
            let angle = second.angle() as f32 - first.angle() as f32 - joint.rest_angle as f32;
            angular_error = match joint.limit_state {
                JointLimitState::AtLower => (angle - joint.lower_limit as f32).min(0.0_f32).abs(),
                JointLimitState::AtUpper => (angle - joint.upper_limit as f32).max(0.0_f32).abs(),
                JointLimitState::Equal => (angle - joint.lower_limit as f32).abs(),
                JointLimitState::Inactive => 0.0_f32,
            };
            let correction = match joint.limit_state {
                JointLimitState::AtLower => (angle - joint.lower_limit as f32 + ANGULAR_SLOP)
                    .clamp(-MAX_ANGULAR_CORRECTION, 0.0_f32),
                JointLimitState::AtUpper => (angle - joint.upper_limit as f32 - ANGULAR_SLOP)
                    .clamp(0.0_f32, MAX_ANGULAR_CORRECTION),
                JointLimitState::Equal => (angle - joint.lower_limit as f32)
                    .clamp(-MAX_ANGULAR_CORRECTION, MAX_ANGULAR_CORRECTION),
                JointLimitState::Inactive => 0.0_f32,
            };
            self.apply_joint_position_impulse(
                joint,
                first,
                second,
                0.0,
                0.0,
                f64::from(-(motor_mass * correction)),
            );
        }

        // The angular limit is applied before anchor offsets are rebuilt.
        let Some(first) = self.scene.get(&joint.first).map(JointBodyState::capture) else {
            return true;
        };
        let Some(second) = self.scene.get(&joint.second).map(JointBodyState::capture) else {
            return true;
        };
        let (r_a, r_b) = joint_anchor_offsets(joint, &first, &second);
        let mass_a = joint.revolute_inverse_mass_first;
        let mass_b = joint.revolute_inverse_mass_second;
        let inertia_a = joint.revolute_inverse_inertia_first;
        let inertia_b = joint.revolute_inverse_inertia_second;
        let matrix = joint_mass_matrix(mass_a, mass_b, inertia_a, inertia_b, r_a, r_b);
        let delta = joint_anchor_delta(&first, &second, r_a, r_b);
        let linear_impulse = solve_symmetric_2x2(
            matrix.0,
            matrix.1,
            matrix.3,
            f64::from(-(delta.0 as f32)),
            f64::from(-(delta.1 as f32)),
        )
        .unwrap_or((0.0, 0.0));
        self.apply_joint_position_impulse(
            joint,
            &first,
            &second,
            linear_impulse.0,
            linear_impulse.1,
            0.0,
        );
        let delta = (delta.0 as f32, delta.1 as f32);
        let linear_error = delta.1.mul_add(delta.1, delta.0 * delta.0).sqrt();
        linear_error <= f32::from_bits(0x3a83_126f) && angular_error <= ANGULAR_SLOP
    }
}
