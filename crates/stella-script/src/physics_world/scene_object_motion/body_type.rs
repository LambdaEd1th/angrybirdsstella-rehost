//! `b2Body::SetType` (`sub_10086B0CC`) and its contact-filter side effect.

use crate::*;

impl SceneObject {
    pub(crate) fn moves_during_step(&self) -> bool {
        self.dynamic_body || self.kinematic_body
    }

    pub(crate) fn has_physics_body(&self) -> bool {
        !matches!(self.collision_shape, CollisionShape::None)
    }

    /// Returns whether the native body type actually changed. The enclosing
    /// world uses that result to flag every attached contact for filtering,
    /// matching SetType's final contact-edge traversal.
    pub(crate) fn set_native_body_type(&mut self, body_type: i32) -> bool {
        let current_body_type = if self.dynamic_body {
            2
        } else if self.kinematic_body {
            1
        } else {
            0
        };
        if current_body_type == body_type || !(0..=2).contains(&body_type) {
            return false;
        }
        let old_center = self.world_center();
        match body_type {
            0 => {
                self.dynamic_body = false;
                self.kinematic_body = false;
            }
            1 => {
                self.dynamic_body = false;
                self.kinematic_body = true;
            }
            2 => {
                self.dynamic_body = true;
                self.kinematic_body = false;
            }
            _ => unreachable!("validated b2Body type"),
        }
        self.reset_native_mass_data(old_center);
        if body_type == 0 {
            self.velocity_x = 0.0;
            self.velocity_y = 0.0;
            self.angular_velocity = 0.0;
        }
        self.force_x = 0.0;
        self.force_y = 0.0;
        self.torque = 0.0;
        self.motion_started = true;
        self.wake();
        true
    }
}

impl RenderBridge {
    pub(crate) fn flag_contacts_for_filtering_for_body(&mut self, body: &str) {
        self.contact_filter_dirty.extend(
            self.broad_phase_contacts
                .iter()
                .filter(|(first, second, _, _)| first == body || second == body)
                .cloned(),
        );
    }
}
