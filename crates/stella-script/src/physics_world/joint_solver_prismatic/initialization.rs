//! `b2PrismaticJoint::InitVelocityConstraints` at `0x1008674D4`.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_prismatic_velocity_constraints(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        step: f64,
    ) {
        self.scale_joint_impulses(&joint.name, step);
        let Some(joint) = self.joints.get(&joint.name).cloned() else {
            return;
        };
        if !joint.is_physical {
            return;
        }
        const TWO_LINEAR_SLOPS: f64 = 0.002;
        let geometry = prismatic_geometry(&joint, first, second);
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
        if let Some(live_joint) = self.joints.get_mut(&joint.name) {
            if new_state != live_joint.limit_state {
                live_joint.limit_impulse = 0.0;
            }
            live_joint.limit_state = new_state;
            if !live_joint.motor_enabled || new_state == JointLimitState::Equal {
                live_joint.motor_impulse = 0.0;
            }
        }
        if let Some(live_joint) = self.joints.get(&joint.name).cloned() {
            self.warm_start_prismatic_joint(&live_joint, first, second);
        }
    }

    fn warm_start_prismatic_joint(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
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
