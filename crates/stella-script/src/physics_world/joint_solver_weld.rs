//! Weld-joint vtable members at `0x100869F54/0x10086A214/0x10086A398`.

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_weld_velocity_constraints<
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
        if joint.is_physical {
            self.apply_joint_velocity_impulse(
                joint,
                first,
                second,
                joint.linear_impulse_x,
                joint.linear_impulse_y,
                joint.angular_impulse,
            );
        }
    }

    pub(crate) fn solve_weld_joint_velocity<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
    ) {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        let matrix = joint_mass_matrix(mass_a, mass_b, inertia_a, inertia_b, r_a, r_b);
        let velocity_a = point_velocity(first, r_a);
        let velocity_b = point_velocity(second, r_b);
        let rhs = (
            f64::from(velocity_a.0 as f32 - velocity_b.0 as f32),
            f64::from(velocity_a.1 as f32 - velocity_b.1 as f32),
            f64::from(first.angular_velocity() as f32 - second.angular_velocity() as f32),
        );
        let impulse = solve_symmetric_3x3(matrix, rhs).unwrap_or((0.0, 0.0, 0.0));
        joint.linear_impulse_x = f64::from(joint.linear_impulse_x as f32 + impulse.0 as f32);
        joint.linear_impulse_y = f64::from(joint.linear_impulse_y as f32 + impulse.1 as f32);
        joint.angular_impulse = f64::from(joint.angular_impulse as f32 + impulse.2 as f32);
        self.apply_joint_velocity_impulse(joint, first, second, impulse.0, impulse.1, impulse.2);
    }

    pub(crate) fn solve_weld_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        let matrix = joint_mass_matrix(mass_a, mass_b, inertia_a, inertia_b, r_a, r_b);
        let delta = joint_anchor_delta(first, second, r_a, r_b);
        // Body angles are continuous; a full turn remains a weld error.
        let angle_error = second.angle() as f32 - first.angle() as f32 - joint.rest_angle as f32;
        let rhs = (
            f64::from(-(delta.0 as f32)),
            f64::from(-(delta.1 as f32)),
            f64::from(-angle_error),
        );
        let impulse = solve_symmetric_3x3(matrix, rhs).unwrap_or((0.0, 0.0, 0.0));
        self.apply_joint_position_impulse(joint, first, second, impulse.0, impulse.1, impulse.2);
        (delta.0 as f32).hypot(delta.1 as f32) <= 0.001_f32
            && angle_error.abs() <= std::f32::consts::PI * (2.0_f32 / 180.0_f32)
    }
}
