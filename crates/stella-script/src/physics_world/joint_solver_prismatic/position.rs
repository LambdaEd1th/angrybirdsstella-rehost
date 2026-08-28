//! `b2PrismaticJoint::SolvePositionConstraints` at `0x100867C68`.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_prismatic_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        const LINEAR_SLOP: f64 = 0.001;
        const TWO_LINEAR_SLOPS: f64 = 0.002;
        const MAX_LINEAR_CORRECTION: f64 = 0.2;
        const ANGULAR_SLOP: f64 = 0.034_906_6;

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
        let mut k22 = inertia_a + inertia_b;
        if k22 == 0.0 {
            // The native matrix writes one into its angular diagonal when
            // both bodies have fixed rotation so the solve remains defined.
            k22 = 1.0;
        }
        let k23 = inertia_a * geometry.a1 + inertia_b * geometry.a2;
        let k33 = mass_a
            + mass_b
            + inertia_a * geometry.a1 * geometry.a1
            + inertia_b * geometry.a2 * geometry.a2;
        let perpendicular_error = geometry.delta.0 * geometry.perpendicular.0
            + geometry.delta.1 * geometry.perpendicular.1;
        let raw_angular_error =
            f64::from(second.angle() as f32 - first.angle() as f32 - joint.rest_angle as f32);
        let angular_error = raw_angular_error;
        let translation = geometry.delta.0 * geometry.axis.0 + geometry.delta.1 * geometry.axis.1;
        // Position constraints recompute limit activity from live translation.
        let (limit_active, raw_limit_error, limit_error) = if !joint.limits_enabled {
            (false, 0.0, 0.0)
        } else if (joint.upper_limit - joint.lower_limit).abs() < TWO_LINEAR_SLOPS {
            (
                true,
                translation.abs(),
                translation.clamp(-MAX_LINEAR_CORRECTION, MAX_LINEAR_CORRECTION),
            )
        } else if translation <= joint.lower_limit {
            let raw = (translation - joint.lower_limit).min(0.0);
            (
                true,
                raw.abs(),
                (raw + LINEAR_SLOP).clamp(-MAX_LINEAR_CORRECTION, 0.0),
            )
        } else if translation >= joint.upper_limit {
            let raw = (translation - joint.upper_limit).max(0.0);
            (
                true,
                raw.abs(),
                (raw - LINEAR_SLOP).clamp(0.0, MAX_LINEAR_CORRECTION),
            )
        } else {
            (false, 0.0, 0.0)
        };
        let (perpendicular_impulse, angular_impulse, axial_impulse) = if limit_active {
            solve_symmetric_3x3(
                (k11, k12, k13, k22, k23, k33),
                (-perpendicular_error, -angular_error, -limit_error),
            )
            .unwrap_or((0.0, 0.0, 0.0))
        } else {
            let impulse = solve_symmetric_2x2(k11, k12, k22, -perpendicular_error, -angular_error)
                .unwrap_or((0.0, 0.0));
            (impulse.0, impulse.1, 0.0)
        };
        self.apply_prismatic_position_impulse(
            joint,
            first,
            second,
            geometry,
            (perpendicular_impulse, axial_impulse, angular_impulse),
        );
        perpendicular_error.abs().max(raw_limit_error) <= LINEAR_SLOP
            && raw_angular_error.abs() <= ANGULAR_SLOP
    }
}
