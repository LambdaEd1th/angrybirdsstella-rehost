//! Weld-joint vtable members at `0x100869F54/0x10086A214/0x10086A398`.

use crate::*;

type NativeWeldMatrix = (f64, f64, f64, f64, f64, f64);

const NATIVE_LINEAR_SLOP: f32 = f32::from_bits(0x3A83_126F);
const NATIVE_ANGULAR_SLOP: f32 = f32::from_bits(0x3D0E_FA36);

fn native_weld_matrix(
    mass_first: f32,
    mass_second: f32,
    inertia_first: f32,
    inertia_second: f32,
    radius_first: (f32, f32),
    radius_second: (f32, f32),
) -> NativeWeldMatrix {
    joint_mass_matrix(
        f64::from(mass_first),
        f64::from(mass_second),
        f64::from(inertia_first),
        f64::from(inertia_second),
        (f64::from(radius_first.0), f64::from(radius_first.1)),
        (f64::from(radius_second.0), f64::from(radius_second.1)),
    )
}

fn apply_cached_weld_velocity_impulse(
    bridge: &mut RenderBridge,
    joint: &PhysicsJoint,
    impulse: (f32, f32),
    angular_impulse: f32,
) {
    let mass_first = joint.weld_inverse_mass_first as f32;
    let mass_second = joint.weld_inverse_mass_second as f32;
    let inertia_first = joint.weld_inverse_inertia_first as f32;
    let inertia_second = joint.weld_inverse_inertia_second as f32;
    let radius_first = (
        joint.weld_radius_first.0 as f32,
        joint.weld_radius_first.1 as f32,
    );
    let radius_second = (
        joint.weld_radius_second.0 as f32,
        joint.weld_radius_second.1 as f32,
    );
    if let Some(object) = bridge.scene.get_mut(&joint.first) {
        object.velocity_x = f64::from((-mass_first).mul_add(impulse.0, object.velocity_x as f32));
        object.velocity_y = f64::from((-mass_first).mul_add(impulse.1, object.velocity_y as f32));
        let cross = (-radius_first.1).mul_add(impulse.0, radius_first.0 * impulse.1);
        object.angular_velocity = f64::from(
            (-inertia_first).mul_add(cross + angular_impulse, object.angular_velocity as f32),
        );
    }
    if let Some(object) = bridge.scene.get_mut(&joint.second) {
        object.velocity_x = f64::from(mass_second.mul_add(impulse.0, object.velocity_x as f32));
        object.velocity_y = f64::from(mass_second.mul_add(impulse.1, object.velocity_y as f32));
        let cross = (-radius_second.1).mul_add(impulse.0, radius_second.0 * impulse.1);
        object.angular_velocity = f64::from(
            inertia_second.mul_add(cross + angular_impulse, object.angular_velocity as f32),
        );
    }
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
        let mass_first = first.inverse_mass_for_solver() as f32;
        let mass_second = second.inverse_mass_for_solver() as f32;
        let inertia_first = first.inverse_inertia() as f32;
        let inertia_second = second.inverse_inertia() as f32;
        joint.weld_radius_first = (f64::from(radius_first.0), f64::from(radius_first.1));
        joint.weld_radius_second = (f64::from(radius_second.0), f64::from(radius_second.1));
        joint.weld_inverse_mass_first = f64::from(mass_first);
        joint.weld_inverse_mass_second = f64::from(mass_second);
        joint.weld_inverse_inertia_first = f64::from(inertia_first);
        joint.weld_inverse_inertia_second = f64::from(inertia_second);
        joint.weld_mass_matrix = native_weld_matrix(
            mass_first,
            mass_second,
            inertia_first,
            inertia_second,
            radius_first,
            radius_second,
        );
        apply_cached_weld_velocity_impulse(
            self,
            joint,
            (joint.linear_impulse_x as f32, joint.linear_impulse_y as f32),
            joint.angular_impulse as f32,
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
        apply_cached_weld_velocity_impulse(self, joint, (-solved.0, -solved.1), -solved.2);
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
        let mass_first = joint.weld_inverse_mass_first as f32;
        let mass_second = joint.weld_inverse_mass_second as f32;
        let inertia_first = joint.weld_inverse_inertia_first as f32;
        let inertia_second = joint.weld_inverse_inertia_second as f32;
        let matrix = native_weld_matrix(
            mass_first,
            mass_second,
            inertia_first,
            inertia_second,
            radius_first,
            radius_second,
        );
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
        let solved = (solved.0 as f32, solved.1 as f32, solved.2 as f32);
        let cross_first = (-radius_first.1).mul_add(solved.0, radius_first.0 * solved.1);
        let cross_second = radius_second
            .1
            .mul_add(solved.0, -radius_second.0 * solved.1);
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.apply_native_position_impulse(
                mass_first,
                (solved.0, solved.1),
                inertia_first,
                solved.2 + cross_first,
            );
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.apply_native_position_impulse(
                -mass_second,
                (solved.0, solved.1),
                inertia_second,
                cross_second - solved.2,
            );
        }
        linear_error <= NATIVE_LINEAR_SLOP && angle_error.abs() <= NATIVE_ANGULAR_SLOP
    }
}
