//! `b2PrismaticJoint::InitVelocityConstraints` at `0x1008674D4`.

use super::{cached_prismatic_geometry, native_prismatic_mass_matrix};
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
        let mass_first = first.inverse_mass_for_solver() as f32;
        let mass_second = second.inverse_mass_for_solver() as f32;
        let inertia_first = first.inverse_inertia() as f32;
        let inertia_second = second.inverse_inertia() as f32;
        joint.prismatic_axis = (
            f64::from(geometry.axis.0 as f32),
            f64::from(geometry.axis.1 as f32),
        );
        joint.prismatic_perpendicular = (
            f64::from(geometry.perpendicular.0 as f32),
            f64::from(geometry.perpendicular.1 as f32),
        );
        joint.prismatic_s1 = f64::from(geometry.s1 as f32);
        joint.prismatic_s2 = f64::from(geometry.s2 as f32);
        joint.prismatic_a1 = f64::from(geometry.a1 as f32);
        joint.prismatic_a2 = f64::from(geometry.a2 as f32);
        joint.prismatic_inverse_mass_first = f64::from(mass_first);
        joint.prismatic_inverse_mass_second = f64::from(mass_second);
        joint.prismatic_inverse_inertia_first = f64::from(inertia_first);
        joint.prismatic_inverse_inertia_second = f64::from(inertia_second);
        (joint.prismatic_mass_matrix, joint.prismatic_motor_mass) = native_prismatic_mass_matrix(
            mass_first,
            mass_second,
            inertia_first,
            inertia_second,
            cached_prismatic_geometry(joint),
        );
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
        self.apply_prismatic_velocity_impulse(
            joint,
            cached_prismatic_geometry(joint),
            (
                joint.linear_impulse_x as f32,
                joint.motor_impulse as f32 + joint.limit_impulse as f32,
                joint.angular_impulse as f32,
            ),
        );
    }
}
