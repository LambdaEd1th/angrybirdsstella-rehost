//! Prismatic constraint impulses mapped through the joint axes.

use crate::*;

impl RenderBridge {
    pub(crate) fn apply_prismatic_velocity_impulse(
        &mut self,
        joint: &PhysicsJoint,
        geometry: PrismaticGeometry,
        impulse: (f32, f32, f32),
    ) {
        let (perpendicular_impulse, axial_impulse, angular_impulse) = impulse;
        let perpendicular = (
            geometry.perpendicular.0 as f32,
            geometry.perpendicular.1 as f32,
        );
        let axis = (geometry.axis.0 as f32, geometry.axis.1 as f32);
        let (s1, s2, a1, a2) = (
            geometry.s1 as f32,
            geometry.s2 as f32,
            geometry.a1 as f32,
            geometry.a2 as f32,
        );
        let impulse = (
            perpendicular
                .0
                .mul_add(perpendicular_impulse, axis.0 * axial_impulse),
            perpendicular
                .1
                .mul_add(perpendicular_impulse, axis.1 * axial_impulse),
        );
        let angular_a = a1.mul_add(
            axial_impulse,
            s1.mul_add(perpendicular_impulse, angular_impulse),
        );
        let angular_b = a2.mul_add(
            axial_impulse,
            s2.mul_add(perpendicular_impulse, angular_impulse),
        );
        let mass_a = joint.prismatic_inverse_mass_first as f32;
        let mass_b = joint.prismatic_inverse_mass_second as f32;
        let inertia_a = joint.prismatic_inverse_inertia_first as f32;
        let inertia_b = joint.prismatic_inverse_inertia_second as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.velocity_x = f64::from((-mass_a).mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y = f64::from((-mass_a).mul_add(impulse.1, object.velocity_y as f32));
            object.angular_velocity =
                f64::from((-inertia_a).mul_add(angular_a, object.angular_velocity as f32));
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.velocity_x = f64::from(mass_b.mul_add(impulse.0, object.velocity_x as f32));
            object.velocity_y = f64::from(mass_b.mul_add(impulse.1, object.velocity_y as f32));
            object.angular_velocity =
                f64::from(inertia_b.mul_add(angular_b, object.angular_velocity as f32));
        }
    }

    pub(crate) fn apply_prismatic_position_impulse(
        &mut self,
        joint: &PhysicsJoint,
        geometry: PrismaticGeometry,
        impulse: (f32, f32, f32),
    ) {
        let (perpendicular_impulse, axial_impulse, angular_impulse) = impulse;
        let perpendicular = (
            geometry.perpendicular.0 as f32,
            geometry.perpendicular.1 as f32,
        );
        let axis = (geometry.axis.0 as f32, geometry.axis.1 as f32);
        let (s1, s2, a1, a2) = (
            geometry.s1 as f32,
            geometry.s2 as f32,
            geometry.a1 as f32,
            geometry.a2 as f32,
        );
        let impulse = (
            perpendicular
                .0
                .mul_add(perpendicular_impulse, axis.0 * axial_impulse),
            perpendicular
                .1
                .mul_add(perpendicular_impulse, axis.1 * axial_impulse),
        );
        let angular_a = a1.mul_add(
            axial_impulse,
            s1.mul_add(perpendicular_impulse, angular_impulse),
        );
        let angular_b = a2.mul_add(
            axial_impulse,
            s2.mul_add(perpendicular_impulse, angular_impulse),
        );
        let mass_a = joint.prismatic_inverse_mass_first as f32;
        let mass_b = joint.prismatic_inverse_mass_second as f32;
        let inertia_a = joint.prismatic_inverse_inertia_first as f32;
        let inertia_b = joint.prismatic_inverse_inertia_second as f32;
        if let Some(object) = self.scene.get_mut(&joint.first) {
            object.apply_native_position_impulse(-mass_a, impulse, -inertia_a, angular_a);
        }
        if let Some(object) = self.scene.get_mut(&joint.second) {
            object.apply_native_position_impulse(mass_b, impulse, inertia_b, angular_b);
        }
    }
}
