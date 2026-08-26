//! `b2RevoluteJoint::InitVelocityConstraints` at `0x100868B90`.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_revolute_velocity_constraints<
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
        // Purple loads these exact Box2D float constants from 0x100A0CA20.
        const TWO_ANGULAR_SLOPS: f32 = f32::from_bits(0x3d8e_fa36);
        let inverse_angular_mass = first.inverse_inertia() as f32 + second.inverse_inertia() as f32;
        let new_state = if !joint.limits_enabled || inverse_angular_mass == 0.0_f32 {
            JointLimitState::Inactive
        } else if (joint.upper_limit as f32 - joint.lower_limit as f32).abs() < TWO_ANGULAR_SLOPS {
            JointLimitState::Equal
        } else {
            let angle = second.angle() as f32 - first.angle() as f32 - joint.rest_angle as f32;
            if angle <= joint.lower_limit as f32 {
                JointLimitState::AtLower
            } else if angle >= joint.upper_limit as f32 {
                JointLimitState::AtUpper
            } else {
                JointLimitState::Inactive
            }
        };
        if new_state != joint.limit_state {
            joint.limit_impulse = 0.0;
        }
        joint.limit_state = new_state;
        if !joint.motor_enabled
            || inverse_angular_mass == 0.0_f32
            || new_state == JointLimitState::Equal
        {
            joint.motor_impulse = 0.0;
        }
        if !joint.is_physical {
            return;
        }
        self.apply_joint_velocity_impulse(
            joint,
            first,
            second,
            joint.linear_impulse_x,
            joint.linear_impulse_y,
            f64::from(joint.motor_impulse as f32 + joint.limit_impulse as f32),
        );
    }
}
