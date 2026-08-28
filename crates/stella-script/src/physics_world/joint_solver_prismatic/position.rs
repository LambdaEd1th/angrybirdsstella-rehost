//! `b2PrismaticJoint::SolvePositionConstraints` at `0x100867C68`.

use super::{
    cached_prismatic_geometry, native_prismatic_mass_matrix, native_solve_2x2, native_solve_3x3,
};
use crate::*;

impl RenderBridge {
    pub(crate) fn solve_prismatic_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        const LINEAR_SLOP: f32 = 0.001;
        const TWO_LINEAR_SLOPS: f32 = 0.002;
        const MAX_LINEAR_CORRECTION: f32 = 0.2;
        const ANGULAR_SLOP: f32 = f32::from_bits(0x3d0e_fa36);

        let geometry = prismatic_geometry(joint, first, second);
        let mass_a = joint.prismatic_inverse_mass_first as f32;
        let mass_b = joint.prismatic_inverse_mass_second as f32;
        let inertia_a = joint.prismatic_inverse_inertia_first as f32;
        let inertia_b = joint.prismatic_inverse_inertia_second as f32;
        let (matrix, _) = native_prismatic_mass_matrix(
            mass_a,
            mass_b,
            inertia_a,
            inertia_b,
            cached_prismatic_geometry(joint),
        );
        joint.prismatic_mass_matrix = matrix;
        let (k11, k12, k13, k22, k23, k33) = (
            matrix.0 as f32,
            matrix.1 as f32,
            matrix.2 as f32,
            matrix.3 as f32,
            matrix.4 as f32,
            matrix.5 as f32,
        );
        let delta = (geometry.delta.0 as f32, geometry.delta.1 as f32);
        let perpendicular = (
            geometry.perpendicular.0 as f32,
            geometry.perpendicular.1 as f32,
        );
        let axis = (geometry.axis.0 as f32, geometry.axis.1 as f32);
        let perpendicular_error = delta.0.mul_add(perpendicular.0, delta.1 * perpendicular.1);
        let raw_angular_error =
            second.angle() as f32 - first.angle() as f32 - joint.rest_angle as f32;
        let angular_error = raw_angular_error;
        let translation = delta.0.mul_add(axis.0, delta.1 * axis.1);
        let lower_limit = joint.lower_limit as f32;
        let upper_limit = joint.upper_limit as f32;
        // Position constraints recompute limit activity from live translation.
        let (limit_active, raw_limit_error, limit_error) = if !joint.limits_enabled {
            (false, 0.0, 0.0)
        } else if (upper_limit - lower_limit).abs() < TWO_LINEAR_SLOPS {
            (
                true,
                translation.abs(),
                translation.clamp(-MAX_LINEAR_CORRECTION, MAX_LINEAR_CORRECTION),
            )
        } else if translation <= lower_limit {
            let raw = (translation - lower_limit).min(0.0);
            (
                true,
                raw.abs(),
                (raw + LINEAR_SLOP).clamp(-MAX_LINEAR_CORRECTION, 0.0),
            )
        } else if translation >= upper_limit {
            let raw = (translation - upper_limit).max(0.0);
            (
                true,
                raw.abs(),
                (raw - LINEAR_SLOP).clamp(0.0, MAX_LINEAR_CORRECTION),
            )
        } else {
            (false, 0.0, 0.0)
        };
        let (perpendicular_impulse, angular_impulse, axial_impulse) = if limit_active {
            native_solve_3x3(
                (k11, k12, k13, k22, k23, k33),
                (-perpendicular_error, -angular_error, -limit_error),
            )
        } else {
            let impulse = native_solve_2x2(k11, k12, k22, -perpendicular_error, -angular_error);
            (impulse.0, impulse.1, 0.0)
        };
        self.apply_prismatic_position_impulse(
            joint,
            geometry,
            (perpendicular_impulse, axial_impulse, angular_impulse),
        );
        perpendicular_error.abs().max(raw_limit_error) <= LINEAR_SLOP
            && raw_angular_error.abs() <= ANGULAR_SLOP
    }
}
