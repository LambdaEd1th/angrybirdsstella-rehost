//! Distance-joint velocity and position constraints.

use crate::*;

const NATIVE_LINEAR_SLOP: f32 = 0.001_f32;

// b2DistanceJoint::InitVelocityConstraints loads this exact word from
// Purple's constant pool at 0x100A0C958.
const NATIVE_TWO_PI: f32 = f32::from_bits(0x40C9_0FDB);

#[derive(Clone, Copy)]
struct NativeDistanceGeometry {
    radius_first: (f32, f32),
    radius_second: (f32, f32),
    delta: (f32, f32),
    length: f32,
}

fn native_distance_geometry<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    joint: &PhysicsJoint,
    first: &F,
    second: &S,
) -> NativeDistanceGeometry {
    let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
    let delta = joint_anchor_delta(first, second, r_a, r_b);
    let r_a = (r_a.0 as f32, r_a.1 as f32);
    let r_b = (r_b.0 as f32, r_b.1 as f32);
    let delta = (delta.0 as f32, delta.1 as f32);
    let length = delta.0.mul_add(delta.0, delta.1 * delta.1).sqrt();
    NativeDistanceGeometry {
        radius_first: r_a,
        radius_second: r_b,
        delta,
        length,
    }
}

fn native_distance_axis(delta: (f32, f32), length: f32) -> (f32, f32) {
    let inverse_length = 1.0_f32 / length;
    (delta.0 * inverse_length, delta.1 * inverse_length)
}

fn native_distance_cross(radius: (f32, f32), axis: (f32, f32)) -> f32 {
    (-radius.1).mul_add(axis.0, radius.0 * axis.1)
}

fn native_distance_mass(
    mass_first: f32,
    mass_second: f32,
    inertia_first: f32,
    inertia_second: f32,
    r_a: (f32, f32),
    r_b: (f32, f32),
    axis: (f32, f32),
) -> (f32, f32) {
    let cross_a = native_distance_cross(r_a, axis);
    let cross_b = native_distance_cross(r_b, axis);
    let mut inverse_mass = (cross_a * cross_a).mul_add(inertia_first, mass_first);
    inverse_mass += mass_second;
    inverse_mass = (cross_b * cross_b).mul_add(inertia_second, inverse_mass);
    let effective_mass = if inverse_mass == 0.0 {
        0.0
    } else {
        1.0_f32 / inverse_mass
    };
    (inverse_mass, effective_mass)
}

impl RenderBridge {
    pub(crate) fn initialize_distance_velocity_constraints<
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

        let geometry = native_distance_geometry(joint, first, second);
        // InitVelocityConstraints uses a wider threshold than Normalize and
        // writes an exactly zero axis for a short distance vector.
        let axis = if geometry.length > NATIVE_LINEAR_SLOP {
            native_distance_axis(geometry.delta, geometry.length)
        } else {
            (0.0, 0.0)
        };
        let mass_first = first.inverse_mass_for_solver() as f32;
        let mass_second = second.inverse_mass_for_solver() as f32;
        let inertia_first = first.inverse_inertia() as f32;
        let inertia_second = second.inverse_inertia() as f32;
        let (inverse_mass, mut effective_mass) = native_distance_mass(
            mass_first,
            mass_second,
            inertia_first,
            inertia_second,
            geometry.radius_first,
            geometry.radius_second,
            axis,
        );
        let mut gamma = 0.0_f32;
        let mut bias = 0.0_f32;
        let frequency = joint.frequency as f32;
        if frequency > 0.0 {
            let step = step as f32;
            let error = geometry.length - joint.rest_length as f32;
            let omega = frequency * NATIVE_TWO_PI;
            let damping = (effective_mass + effective_mass) * joint.damping_ratio as f32;
            let stiffness = effective_mass * (omega * omega);
            let denominator = step * omega.mul_add(damping, stiffness * step);
            gamma = if denominator == 0.0 {
                0.0
            } else {
                1.0_f32 / denominator
            };
            bias = stiffness * (error * step) * gamma;
            let softened_inverse_mass = inverse_mass + gamma;
            effective_mass = if softened_inverse_mass == 0.0 {
                0.0
            } else {
                1.0_f32 / softened_inverse_mass
            };
        }

        joint.distance_radius_first = (
            f64::from(geometry.radius_first.0),
            f64::from(geometry.radius_first.1),
        );
        joint.distance_radius_second = (
            f64::from(geometry.radius_second.0),
            f64::from(geometry.radius_second.1),
        );
        joint.distance_inverse_mass_first = f64::from(mass_first);
        joint.distance_inverse_mass_second = f64::from(mass_second);
        joint.distance_inverse_inertia_first = f64::from(inertia_first);
        joint.distance_inverse_inertia_second = f64::from(inertia_second);
        joint.distance_current_length = f64::from(geometry.length);
        joint.distance_axis = (f64::from(axis.0), f64::from(axis.1));
        joint.distance_effective_mass = f64::from(effective_mass);
        joint.distance_gamma = f64::from(gamma);
        joint.distance_bias = f64::from(bias);

        let impulse = joint.distance_impulse as f32;
        self.apply_cached_distance_velocity_impulse(
            joint,
            geometry.radius_first,
            geometry.radius_second,
            (axis.0 * impulse, axis.1 * impulse),
        );
    }

    pub(crate) fn solve_distance_joint_velocity<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &mut PhysicsJoint,
        first: &F,
        second: &S,
        _step: f64,
    ) {
        let r_a = (
            joint.distance_radius_first.0 as f32,
            joint.distance_radius_first.1 as f32,
        );
        let r_b = (
            joint.distance_radius_second.0 as f32,
            joint.distance_radius_second.1 as f32,
        );
        let axis = (joint.distance_axis.0 as f32, joint.distance_axis.1 as f32);
        let velocity_a = point_velocity(first, (f64::from(r_a.0), f64::from(r_a.1)));
        let velocity_b = point_velocity(second, (f64::from(r_b.0), f64::from(r_b.1)));
        let relative_x = velocity_b.0 as f32 - velocity_a.0 as f32;
        let relative_y = velocity_b.1 as f32 - velocity_a.1 as f32;
        let relative_speed = axis.0.mul_add(relative_x, axis.1 * relative_y);
        let old_impulse = joint.distance_impulse as f32;
        let mut velocity_error = joint.distance_bias as f32 + relative_speed;
        velocity_error = (joint.distance_gamma as f32).mul_add(old_impulse, velocity_error);
        let impulse_product = joint.distance_effective_mass as f32 * velocity_error;
        let impulse = -impulse_product;
        joint.distance_impulse = f64::from(old_impulse - impulse_product);
        self.apply_cached_distance_velocity_impulse(
            joint,
            r_a,
            r_b,
            (axis.0 * impulse, axis.1 * impulse),
        );
    }

    pub(crate) fn solve_distance_joint_position<
        F: JointBodyView + ?Sized,
        S: JointBodyView + ?Sized,
    >(
        &mut self,
        joint: &PhysicsJoint,
        first: &F,
        second: &S,
    ) -> bool {
        if joint.frequency as f32 > 0.0 {
            return true;
        }
        let geometry = native_distance_geometry(joint, first, second);
        // Normalize returns zero below FLT_EPSILON without changing the
        // vector. Keep the tiny unnormalized direction in that branch.
        let (axis, normalized_length) = if geometry.length >= f32::EPSILON {
            (
                native_distance_axis(geometry.delta, geometry.length),
                geometry.length,
            )
        } else {
            (geometry.delta, 0.0)
        };
        let error = (normalized_length - joint.rest_length as f32).clamp(-0.2_f32, 0.2_f32);
        let impulse = -(joint.distance_effective_mass as f32 * error);
        self.apply_cached_distance_position_impulse(
            joint,
            geometry.radius_first,
            geometry.radius_second,
            (axis.0 * impulse, axis.1 * impulse),
        );
        error.abs() < NATIVE_LINEAR_SLOP
    }
}
