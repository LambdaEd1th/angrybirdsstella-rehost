//! `b2RevoluteJoint::InitVelocityConstraints` at `0x100868B90`.

use super::apply_cached_revolute_velocity_impulse;
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
        if !joint.has_native_joint() {
            return;
        }
        let (radius_first, radius_second) = joint_anchor_offsets(joint, first, second);
        let radius_first = (radius_first.0 as f32, radius_first.1 as f32);
        let radius_second = (radius_second.0 as f32, radius_second.1 as f32);
        let mass_first = first.inverse_mass_for_solver() as f32;
        let mass_second = second.inverse_mass_for_solver() as f32;
        let inertia_first = first.inverse_inertia() as f32;
        let inertia_second = second.inverse_inertia() as f32;
        joint.revolute_radius_first = (f64::from(radius_first.0), f64::from(radius_first.1));
        joint.revolute_radius_second = (f64::from(radius_second.0), f64::from(radius_second.1));
        joint.revolute_inverse_mass_first = f64::from(mass_first);
        joint.revolute_inverse_mass_second = f64::from(mass_second);
        joint.revolute_inverse_inertia_first = f64::from(inertia_first);
        joint.revolute_inverse_inertia_second = f64::from(inertia_second);
        joint.revolute_mass_matrix = joint_mass_matrix(
            f64::from(mass_first),
            f64::from(mass_second),
            f64::from(inertia_first),
            f64::from(inertia_second),
            (f64::from(radius_first.0), f64::from(radius_first.1)),
            (f64::from(radius_second.0), f64::from(radius_second.1)),
        );
        // Purple loads these exact Box2D float constants from 0x100A0CA20.
        const TWO_ANGULAR_SLOPS: f32 = f32::from_bits(0x3d8e_fa36);
        let inverse_angular_mass = inertia_first + inertia_second;
        joint.revolute_motor_mass = f64::from(if inverse_angular_mass > 0.0_f32 {
            inverse_angular_mass.recip()
        } else {
            inverse_angular_mass
        });
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
        apply_cached_revolute_velocity_impulse(
            self,
            joint,
            (joint.linear_impulse_x as f32, joint.linear_impulse_y as f32),
            joint.motor_impulse as f32 + joint.limit_impulse as f32,
        );
    }
}
