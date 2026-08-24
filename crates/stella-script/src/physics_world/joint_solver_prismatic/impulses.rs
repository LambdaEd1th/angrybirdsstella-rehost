//! Prismatic constraint impulses mapped through the joint axes.

use crate::*;

impl RenderBridge {
    pub(crate) fn apply_prismatic_velocity_impulse(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        geometry: PrismaticGeometry,
        impulse: (f64, f64, f64),
    ) {
        let (perpendicular_impulse, axial_impulse, angular_impulse) = impulse;
        let impulse = (
            geometry.perpendicular.0 * perpendicular_impulse + geometry.axis.0 * axial_impulse,
            geometry.perpendicular.1 * perpendicular_impulse + geometry.axis.1 * axial_impulse,
        );
        let angular_a =
            geometry.s1 * perpendicular_impulse + geometry.a1 * axial_impulse + angular_impulse;
        let angular_b =
            geometry.s2 * perpendicular_impulse + geometry.a2 * axial_impulse + angular_impulse;
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.velocity_x -= mass_a * impulse.0;
            object.velocity_y -= mass_a * impulse.1;
            object.angular_velocity -= inertia_a * angular_a;
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.velocity_x += mass_b * impulse.0;
            object.velocity_y += mass_b * impulse.1;
            object.angular_velocity += inertia_b * angular_b;
        }
    }

    pub(crate) fn apply_prismatic_position_impulse(
        &mut self,
        joint: &PhysicsJoint,
        first: &SceneObject,
        second: &SceneObject,
        geometry: PrismaticGeometry,
        impulse: (f64, f64, f64),
    ) {
        let (perpendicular_impulse, axial_impulse, angular_impulse) = impulse;
        let impulse = (
            geometry.perpendicular.0 * perpendicular_impulse + geometry.axis.0 * axial_impulse,
            geometry.perpendicular.1 * perpendicular_impulse + geometry.axis.1 * axial_impulse,
        );
        let angular_a =
            geometry.s1 * perpendicular_impulse + geometry.a1 * axial_impulse + angular_impulse;
        let angular_b =
            geometry.s2 * perpendicular_impulse + geometry.a2 * axial_impulse + angular_impulse;
        let mass_a = first.inverse_mass_for_solver();
        let mass_b = second.inverse_mass_for_solver();
        let inertia_a = first.inverse_inertia();
        let inertia_b = second.inverse_inertia();
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.apply_position_delta(
                -mass_a * impulse.0,
                -mass_a * impulse.1,
                -inertia_a * angular_a,
            );
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.apply_position_delta(
                mass_b * impulse.0,
                mass_b * impulse.1,
                inertia_b * angular_b,
            );
        }
    }
}
