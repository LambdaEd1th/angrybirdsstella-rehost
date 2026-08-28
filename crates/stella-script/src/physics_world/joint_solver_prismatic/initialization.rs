//! `b2PrismaticJoint::InitVelocityConstraints` at `0x1008674D4`.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_prismatic_velocity_constraints<
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
        const TWO_LINEAR_SLOPS: f32 = 0.002;
        let geometry = prismatic_geometry(joint, first, second);
        let delta = (geometry.delta.0 as f32, geometry.delta.1 as f32);
        let axis = (geometry.axis.0 as f32, geometry.axis.1 as f32);
        let translation = delta.0.mul_add(axis.0, delta.1 * axis.1);
        let lower_limit = joint.lower_limit as f32;
        let upper_limit = joint.upper_limit as f32;
        let new_state = if !joint.limits_enabled {
            JointLimitState::Inactive
        } else if (upper_limit - lower_limit).abs() < TWO_LINEAR_SLOPS {
            JointLimitState::Equal
        } else if translation <= lower_limit {
            JointLimitState::AtLower
        } else if translation >= upper_limit {
            JointLimitState::AtUpper
        } else {
            JointLimitState::Inactive
        };
        if new_state != joint.limit_state {
            joint.limit_impulse = 0.0;
        }
        joint.limit_state = new_state;
        if !joint.motor_enabled || new_state == JointLimitState::Equal {
            joint.motor_impulse = 0.0;
        }
        self.warm_start_prismatic_joint(joint, first, second);
    }

    fn warm_start_prismatic_joint<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) {
        let geometry = prismatic_geometry(joint, first, second);
        self.apply_prismatic_velocity_impulse(
            joint,
            first,
            second,
            geometry,
            (
                joint.linear_impulse_x as f32,
                joint.motor_impulse as f32 + joint.limit_impulse as f32,
                joint.angular_impulse as f32,
            ),
        );
    }
}
