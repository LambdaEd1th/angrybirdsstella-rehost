//! Revolute-joint members recovered from its Box2D vtable.

mod initialization;
mod position;
mod velocity;

use crate::*;

pub(crate) fn apply_cached_revolute_velocity_impulse(
    bridge: &mut RenderBridge,
    joint: &PhysicsJoint,
    impulse: (f32, f32),
    angular_impulse: f32,
) {
    let mass_first = joint.revolute_inverse_mass_first as f32;
    let mass_second = joint.revolute_inverse_mass_second as f32;
    let inertia_first = joint.revolute_inverse_inertia_first as f32;
    let inertia_second = joint.revolute_inverse_inertia_second as f32;
    let radius_first = (
        joint.revolute_radius_first.0 as f32,
        joint.revolute_radius_first.1 as f32,
    );
    let radius_second = (
        joint.revolute_radius_second.0 as f32,
        joint.revolute_radius_second.1 as f32,
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
