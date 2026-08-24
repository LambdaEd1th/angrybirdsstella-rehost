//! Native contact listener records and endpoint wake transition.

use crate::*;

impl RenderBridge {
    pub(crate) fn wake_contact_bodies(&mut self, contact_key: &ContactKey) {
        for name in [&contact_key.0, &contact_key.1] {
            if let Some(object) = self.scene.get_mut(name) {
                object.wake();
            }
        }
    }

    pub(crate) fn native_contact_event(
        contact_key: &ContactKey,
        first: &SceneObject,
        second: &SceneObject,
        manifold: ContactManifold,
        sensor: bool,
        began: bool,
    ) -> ContactEvent {
        ContactEvent {
            first: contact_key.0.clone(),
            second: contact_key.1.clone(),
            first_fixture: contact_key.2,
            second_fixture: contact_key.3,
            sensor,
            began,
            ended: false,
            impulse: 0.0,
            normal_x: manifold.normal_x,
            normal_y: manifold.normal_y,
            point_x: manifold.point_x,
            point_y: manifold.point_y,
            first_mass: if first.dynamic_body {
                f64::from(first.body_mass)
            } else {
                0.0
            },
            first_velocity_x: first.velocity_x,
            first_velocity_y: first.velocity_y,
            second_mass: if second.dynamic_body {
                f64::from(second.body_mass)
            } else {
                0.0
            },
            second_velocity_x: second.velocity_x,
            second_velocity_y: second.velocity_y,
        }
    }

    pub(crate) fn native_contact_end_event(contact_key: &ContactKey, sensor: bool) -> ContactEvent {
        ContactEvent {
            first: contact_key.0.clone(),
            second: contact_key.1.clone(),
            first_fixture: contact_key.2,
            second_fixture: contact_key.3,
            sensor,
            began: false,
            ended: true,
            impulse: 0.0,
            normal_x: 0.0,
            normal_y: 0.0,
            point_x: 0.0,
            point_y: 0.0,
            first_mass: 0.0,
            first_velocity_x: 0.0,
            first_velocity_y: 0.0,
            second_mass: 0.0,
            second_velocity_x: 0.0,
            second_velocity_y: 0.0,
        }
    }
}
