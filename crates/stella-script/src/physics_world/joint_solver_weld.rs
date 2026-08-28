//! Weld-joint vtable members at `0x100869F54/0x10086A214/0x10086A398`.

use crate::*;

type NativeWeldMatrix = (f64, f64, f64, f64, f64, f64);

const NATIVE_LINEAR_SLOP: f32 = f32::from_bits(0x3A83_126F);
const NATIVE_ANGULAR_SLOP: f32 = f32::from_bits(0x3D0E_FA36);

fn native_weld_matrix<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    first: &F,
    second: &S,
    radius_first: (f32, f32),
    radius_second: (f32, f32),
) -> NativeWeldMatrix {
    joint_mass_matrix(
        first.inverse_mass_for_solver(),
        second.inverse_mass_for_solver(),
        first.inverse_inertia(),
        second.inverse_inertia(),
        (f64::from(radius_first.0), f64::from(radius_first.1)),
        (f64::from(radius_second.0), f64::from(radius_second.1)),
    )
}

fn native_weld_velocity_error<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    first: &F,
    second: &S,
    radius_first: (f32, f32),
    radius_second: (f32, f32),
) -> (f64, f64, f64) {
    let velocity_first = first.velocity();
    let velocity_second = second.velocity();
    let velocity_first = (velocity_first.0 as f32, velocity_first.1 as f32);
    let velocity_second = (velocity_second.0 as f32, velocity_second.1 as f32);
    let angular_first = first.angular_velocity() as f32;
    let angular_second = second.angular_velocity() as f32;

    // Exact sub_10086A278..0x10086A2A8 order: build the body-B point
    // velocity, subtract body-A linear velocity, then add body-A rotation.
    let mut relative_x = (-angular_second).mul_add(radius_second.1, velocity_second.0);
    let mut relative_y = angular_second.mul_add(radius_second.0, velocity_second.1);
    relative_x -= velocity_first.0;
    relative_y -= velocity_first.1;
    relative_x = angular_first.mul_add(radius_first.1, relative_x);
    relative_y = (-angular_first).mul_add(radius_first.0, relative_y);
    (
        f64::from(relative_x),
        f64::from(relative_y),
        f64::from(angular_second - angular_first),
    )
}

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
        if !joint.is_physical {
            return;
        }
        let (radius_first, radius_second) = joint_anchor_offsets(joint, first, second);
        let radius_first = (radius_first.0 as f32, radius_first.1 as f32);
        let radius_second = (radius_second.0 as f32, radius_second.1 as f32);
        joint.weld_radius_first = (f64::from(radius_first.0), f64::from(radius_first.1));
        joint.weld_radius_second = (f64::from(radius_second.0), f64::from(radius_second.1));
        joint.weld_mass_matrix = native_weld_matrix(first, second, radius_first, radius_second);
        self.apply_joint_velocity_impulse(
            joint,
            first,
            second,
            joint.linear_impulse_x,
            joint.linear_impulse_y,
            joint.angular_impulse,
        );
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
        let radius_first = (
            joint.weld_radius_first.0 as f32,
            joint.weld_radius_first.1 as f32,
        );
        let radius_second = (
            joint.weld_radius_second.0 as f32,
            joint.weld_radius_second.1 as f32,
        );
        let velocity_error = native_weld_velocity_error(first, second, radius_first, radius_second);
        let solved =
            solve_symmetric_3x3(joint.weld_mass_matrix, velocity_error).unwrap_or((0.0, 0.0, 0.0));
        let solved = (solved.0 as f32, solved.1 as f32, solved.2 as f32);
        joint.linear_impulse_x = f64::from(joint.linear_impulse_x as f32 - solved.0);
        joint.linear_impulse_y = f64::from(joint.linear_impulse_y as f32 - solved.1);
        joint.angular_impulse = f64::from(joint.angular_impulse as f32 - solved.2);
        self.apply_joint_velocity_impulse(
            joint,
            first,
            second,
            f64::from(-solved.0),
            f64::from(-solved.1),
            f64::from(-solved.2),
        );
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
        let (radius_first, radius_second) = joint_anchor_offsets(joint, first, second);
        let radius_first = (radius_first.0 as f32, radius_first.1 as f32);
        let radius_second = (radius_second.0 as f32, radius_second.1 as f32);
        let matrix = native_weld_matrix(first, second, radius_first, radius_second);
        let delta = joint_anchor_delta(
            first,
            second,
            (f64::from(radius_first.0), f64::from(radius_first.1)),
            (f64::from(radius_second.0), f64::from(radius_second.1)),
        );
        let delta = (delta.0 as f32, delta.1 as f32);
        let angle_error = (second.angle() as f32 - first.angle() as f32) - joint.rest_angle as f32;
        let linear_error = delta.0.mul_add(delta.0, delta.1 * delta.1).sqrt();
        let solved = solve_symmetric_3x3(
            matrix,
            (
                f64::from(delta.0),
                f64::from(delta.1),
                f64::from(angle_error),
            ),
        )
        .unwrap_or((0.0, 0.0, 0.0));
        self.apply_joint_position_impulse(
            joint,
            first,
            second,
            f64::from(-(solved.0 as f32)),
            f64::from(-(solved.1 as f32)),
            f64::from(-(solved.2 as f32)),
        );
        linear_error <= NATIVE_LINEAR_SLOP && angle_error.abs() <= NATIVE_ANGULAR_SLOP
    }
}
