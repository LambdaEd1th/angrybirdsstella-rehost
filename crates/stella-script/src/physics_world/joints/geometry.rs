//! Shared anchor, axis and point-velocity geometry used by joint subclasses.

use crate::{JointBodyView, RenderBridge};

use super::model::PhysicsJoint;

pub(crate) fn cross_2d(first: (f64, f64), second: (f64, f64)) -> f64 {
    first.0 * second.1 - first.1 * second.0
}

pub(crate) fn trace_physics_body(bridge: &RenderBridge, phase: &str) {
    let Ok(name) = std::env::var("STELLA_TRACE_BODY") else {
        return;
    };
    let Some(object) = bridge.scene.get(&name) else {
        return;
    };
    eprintln!(
        "physics-body {phase} {name:?} position=({}, {}) velocity=({}, {}) angle={} angular={} sleeping={}",
        object.x,
        object.y,
        object.velocity_x,
        object.velocity_y,
        object.angle,
        object.angular_velocity,
        object.sleeping
    );
}

pub(crate) fn joint_anchor_offsets<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    joint: &PhysicsJoint,
    first: &F,
    second: &S,
) -> ((f64, f64), (f64, f64)) {
    let first_anchor = first
        .native_transform_body_point((joint.first_anchor.0 as f32, joint.first_anchor.1 as f32));
    let second_anchor = second
        .native_transform_body_point((joint.second_anchor.0 as f32, joint.second_anchor.1 as f32));
    let first_center = first.native_world_center();
    let second_center = second.native_world_center();
    (
        (
            f64::from(first_anchor.0 - first_center.0),
            f64::from(first_anchor.1 - first_center.1),
        ),
        (
            f64::from(second_anchor.0 - second_center.0),
            f64::from(second_anchor.1 - second_center.1),
        ),
    )
}

pub(crate) fn joint_anchor_delta<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    first: &F,
    second: &S,
    first_offset: (f64, f64),
    second_offset: (f64, f64),
) -> (f64, f64) {
    let first_center = first.native_world_center();
    let second_center = second.native_world_center();
    let first_offset = (first_offset.0 as f32, first_offset.1 as f32);
    let second_offset = (second_offset.0 as f32, second_offset.1 as f32);
    (
        f64::from((second_center.0 + second_offset.0) - first_center.0 - first_offset.0),
        f64::from((second_center.1 + second_offset.1) - first_center.1 - first_offset.1),
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PrismaticGeometry {
    pub(crate) delta: (f64, f64),
    pub(crate) axis: (f64, f64),
    pub(crate) perpendicular: (f64, f64),
    pub(crate) a1: f64,
    pub(crate) a2: f64,
    pub(crate) s1: f64,
    pub(crate) s2: f64,
}

pub(crate) fn inverse_rotate_vector(vector: (f64, f64), angle: f64) -> (f64, f64) {
    let cosine = angle.cos();
    let sine = angle.sin();
    (
        cosine * vector.0 + sine * vector.1,
        -sine * vector.0 + cosine * vector.1,
    )
}

pub(crate) fn prismatic_geometry<F: JointBodyView + ?Sized, S: JointBodyView + ?Sized>(
    joint: &PhysicsJoint,
    first: &F,
    second: &S,
) -> PrismaticGeometry {
    let (r_a, r_b) = joint_anchor_offsets(joint, first, second);
    let delta = joint_anchor_delta(first, second, r_a, r_b);
    let axis = native_rotate_vector(joint.local_axis, first.angle());
    let r_a = (r_a.0 as f32, r_a.1 as f32);
    let r_b = (r_b.0 as f32, r_b.1 as f32);
    let delta = (delta.0 as f32, delta.1 as f32);
    let axis = (axis.0 as f32, axis.1 as f32);
    let perpendicular = (-axis.1, axis.0);
    let delta_plus_r_a = (delta.0 + r_a.0, delta.1 + r_a.1);
    PrismaticGeometry {
        delta: (f64::from(delta.0), f64::from(delta.1)),
        axis: (f64::from(axis.0), f64::from(axis.1)),
        perpendicular: (f64::from(perpendicular.0), f64::from(perpendicular.1)),
        a1: f64::from(native_cross_2d(delta_plus_r_a, axis)),
        a2: f64::from(native_cross_2d(r_b, axis)),
        s1: f64::from(native_cross_2d(delta_plus_r_a, perpendicular)),
        s2: f64::from(native_cross_2d(r_b, perpendicular)),
    }
}

fn native_cross_2d(first: (f32, f32), second: (f32, f32)) -> f32 {
    (-first.1).mul_add(second.0, first.0 * second.1)
}

fn native_rotate_vector(vector: (f64, f64), angle: f64) -> (f64, f64) {
    let vector = (vector.0 as f32, vector.1 as f32);
    let (sine, cosine) = (angle as f32).sin_cos();
    let sine_y = sine * vector.1;
    let sine_x = sine * vector.0;
    (
        f64::from(cosine.mul_add(vector.0, -sine_y)),
        f64::from(cosine.mul_add(vector.1, sine_x)),
    )
}

pub(crate) fn point_velocity(
    object: &(impl JointBodyView + ?Sized),
    radius: (f64, f64),
) -> (f64, f64) {
    let angular_velocity = object.angular_velocity() as f32;
    let velocity = object.velocity();
    let radius = (radius.0 as f32, radius.1 as f32);
    (
        f64::from((-angular_velocity).mul_add(radius.1, velocity.0 as f32)),
        f64::from(angular_velocity.mul_add(radius.0, velocity.1 as f32)),
    )
}
