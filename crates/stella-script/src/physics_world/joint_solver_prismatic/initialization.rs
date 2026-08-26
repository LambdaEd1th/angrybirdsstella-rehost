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
        const TWO_LINEAR_SLOPS: f64 = 0.002;
        let geometry = prismatic_geometry(joint, first, second);
        let translation = geometry.delta.0 * geometry.axis.0 + geometry.delta.1 * geometry.axis.1;
        let new_state = if !joint.limits_enabled {
            JointLimitState::Inactive
        } else if (joint.upper_limit - joint.lower_limit).abs() < TWO_LINEAR_SLOPS {
            JointLimitState::Equal
        } else if translation <= joint.lower_limit {
            JointLimitState::AtLower
        } else if translation >= joint.upper_limit {
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
                joint.linear_impulse_x,
                joint.motor_impulse + joint.limit_impulse,
                joint.angular_impulse,
            ),
        );
    }
}
