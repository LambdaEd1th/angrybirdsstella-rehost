//! Box2D island position integration and translation/rotation clamps.

use crate::*;

impl RenderBridge {
    #[cfg(test)]
    pub(crate) fn integrate_positions(
        &mut self,
        step: f64,
        max_translation: f64,
        max_rotation: f64,
    ) {
        let body_names = self.scene.keys().cloned().collect::<Vec<_>>();
        self.integrate_island_positions(&body_names, step, max_translation, max_rotation);
    }

    pub(crate) fn integrate_island_positions(
        &mut self,
        body_names: &[String],
        step: f64,
        max_translation: f64,
        max_rotation: f64,
    ) {
        let step = step as f32;
        let max_translation = max_translation as f32;
        let max_rotation = max_rotation as f32;
        for name in body_names {
            let Some(object) = self.scene.get_mut(name) else {
                continue;
            };
            if !object.moves_during_step()
                || !object.active
                || !object.motion_started
                || object.sleeping
            {
                continue;
            }
            let mut velocity_x = object.velocity_x as f32;
            let mut velocity_y = object.velocity_y as f32;
            let mut angular_velocity = object.angular_velocity as f32;
            let translation_x = step * velocity_x;
            let translation_y = step * velocity_y;
            let translation_squared =
                translation_x.mul_add(translation_x, translation_y * translation_y);
            if translation_squared > max_translation * max_translation {
                let scale = max_translation / translation_squared.sqrt();
                velocity_x *= scale;
                velocity_y *= scale;
            }
            let mut rotation = step * angular_velocity;
            if rotation * rotation > max_rotation * max_rotation {
                angular_velocity *= max_rotation / rotation.abs();
                rotation = step * angular_velocity;
            }

            object.velocity_x = f64::from(velocity_x);
            object.velocity_y = f64::from(velocity_y);
            object.angular_velocity = f64::from(angular_velocity);
            object.apply_native_position_delta(step * velocity_x, step * velocity_y, rotation);

            if ![
                object.x,
                object.y,
                object.velocity_x,
                object.velocity_y,
                object.angle,
                object.angular_velocity,
            ]
            .into_iter()
            .all(f64::is_finite)
            {
                object.sleeping = true;
                object.velocity_x = 0.0;
                object.velocity_y = 0.0;
                object.angular_velocity = 0.0;
            }
        }
    }
}
