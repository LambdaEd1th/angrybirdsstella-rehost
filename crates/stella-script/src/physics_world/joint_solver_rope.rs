//! Rope-joint vtable members at `0x10086983C/0x100869B30/0x100869C48`.

use crate::*;

const NATIVE_LINEAR_SLOP: f32 = 0.001_f32;

#[derive(Clone, Copy)]
struct NativeRopeGeometry {
    radius_first: (f32, f32),
    radius_second: (f32, f32),
    delta: (f32, f32),
    length: f32,
}

fn native_rope_geometry<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    joint: &PhysicsJoint,
    first: &F,
    second: &S,
) -> NativeRopeGeometry {
    let (radius_first, radius_second) = joint_anchor_offsets(joint, first, second);
    let delta = joint_anchor_delta(first, second, radius_first, radius_second);
    let radius_first = (radius_first.0 as f32, radius_first.1 as f32);
    let radius_second = (radius_second.0 as f32, radius_second.1 as f32);
    let delta = (delta.0 as f32, delta.1 as f32);
    let length = delta.0.mul_add(delta.0, delta.1 * delta.1).sqrt();
    NativeRopeGeometry {
        radius_first,
        radius_second,
        delta,
        length,
    }
}

fn native_rope_axis(delta: (f32, f32), length: f32) -> (f32, f32) {
    let inverse_length = 1.0_f32 / length;
    (inverse_length * delta.0, inverse_length * delta.1)
}

fn native_rope_cross(radius: (f32, f32), axis: (f32, f32)) -> f32 {
    (-radius.1).mul_add(axis.0, radius.0 * axis.1)
}

fn native_rope_effective_mass(
    mass_first: f32,
    mass_second: f32,
    inertia_first: f32,
    inertia_second: f32,
    radius_first: (f32, f32),
    radius_second: (f32, f32),
    axis: (f32, f32),
) -> f32 {
    let cross_first = native_rope_cross(radius_first, axis);
    let cross_second = native_rope_cross(radius_second, axis);
    let mut inverse_mass = (cross_first * cross_first).mul_add(inertia_first, mass_first);
    inverse_mass += mass_second;
    inverse_mass = (cross_second * cross_second).mul_add(inertia_second, inverse_mass);
    if inverse_mass == 0.0 {
        0.0
    } else {
        1.0_f32 / inverse_mass
    }
}

impl RenderBridge {
    pub(crate) fn initialize_rope_velocity_constraints<
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
        let geometry = native_rope_geometry(joint, first, second);
        let mass_first = first.inverse_mass_for_solver() as f32;
        let mass_second = second.inverse_mass_for_solver() as f32;
        let inertia_first = first.inverse_inertia() as f32;
        let inertia_second = second.inverse_inertia() as f32;
        joint.distance_inverse_mass_first = f64::from(mass_first);
        joint.distance_inverse_mass_second = f64::from(mass_second);
        joint.distance_inverse_inertia_first = f64::from(inertia_first);
        joint.distance_inverse_inertia_second = f64::from(inertia_second);
        joint.distance_current_length = f64::from(geometry.length);
        joint.limit_state = if geometry.length - joint.rest_length as f32 > 0.0 {
            JointLimitState::AtUpper
        } else {
            JointLimitState::Inactive
        };
        joint.distance_radius_first = (
            f64::from(geometry.radius_first.0),
            f64::from(geometry.radius_first.1),
        );
        joint.distance_radius_second = (
            f64::from(geometry.radius_second.0),
            f64::from(geometry.radius_second.1),
        );
        if geometry.length <= NATIVE_LINEAR_SLOP {
            joint.distance_axis = (0.0, 0.0);
            joint.distance_effective_mass = 0.0;
            joint.distance_impulse = 0.0;
            return;
        }

        let axis = native_rope_axis(geometry.delta, geometry.length);
        let effective_mass = native_rope_effective_mass(
            mass_first,
            mass_second,
            inertia_first,
            inertia_second,
            geometry.radius_first,
            geometry.radius_second,
            axis,
        );
        joint.distance_axis = (f64::from(axis.0), f64::from(axis.1));
        joint.distance_effective_mass = f64::from(effective_mass);

        let impulse = joint.distance_impulse as f32;
        self.apply_cached_distance_velocity_impulse(
            joint,
            geometry.radius_first,
            geometry.radius_second,
            (axis.0 * impulse, axis.1 * impulse),
        );
    }

    pub(crate) fn solve_rope_joint_velocity<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
        step: f64,
    ) {
        let radius_first = (
            joint.distance_radius_first.0 as f32,
            joint.distance_radius_first.1 as f32,
        );
        let radius_second = (
            joint.distance_radius_second.0 as f32,
            joint.distance_radius_second.1 as f32,
        );
        let axis = (joint.distance_axis.0 as f32, joint.distance_axis.1 as f32);
        let velocity_first = point_velocity(
            first,
            (f64::from(radius_first.0), f64::from(radius_first.1)),
        );
        let velocity_second = point_velocity(
            second,
            (f64::from(radius_second.0), f64::from(radius_second.1)),
        );
        let relative_x = velocity_second.0 as f32 - velocity_first.0 as f32;
        let relative_y = velocity_second.1 as f32 - velocity_first.1 as f32;
        let mut relative_speed = axis.0.mul_add(relative_x, axis.1 * relative_y);
        let length_error = joint.distance_current_length as f32 - joint.rest_length as f32;
        if length_error < 0.0 {
            let step = step as f32;
            let inverse_step = if step > 0.0 { 1.0_f32 / step } else { 0.0 };
            relative_speed = length_error.mul_add(inverse_step, relative_speed);
        }
        let old_impulse = joint.distance_impulse as f32;
        let candidate =
            (-relative_speed).mul_add(joint.distance_effective_mass as f32, old_impulse);
        let new_impulse = candidate.min(0.0);
        joint.distance_impulse = f64::from(new_impulse);
        let impulse_delta = new_impulse - old_impulse;
        self.apply_cached_distance_velocity_impulse(
            joint,
            radius_first,
            radius_second,
            (axis.0 * impulse_delta, axis.1 * impulse_delta),
        );
    }

    pub(crate) fn solve_rope_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        let geometry = native_rope_geometry(joint, first, second);
        let (axis, normalized_length) = if geometry.length >= f32::EPSILON {
            (
                native_rope_axis(geometry.delta, geometry.length),
                geometry.length,
            )
        } else {
            (geometry.delta, 0.0)
        };
        let raw_error = normalized_length - joint.rest_length as f32;
        let correction = raw_error.clamp(0.0, 0.2_f32);
        let impulse = -(joint.distance_effective_mass as f32 * correction);
        self.apply_cached_distance_position_impulse(
            joint,
            geometry.radius_first,
            geometry.radius_second,
            (axis.0 * impulse, axis.1 * impulse),
        );
        raw_error < NATIVE_LINEAR_SLOP
    }
}
