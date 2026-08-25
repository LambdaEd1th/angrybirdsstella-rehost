//! SceneObject motion reporting and bounce stages inside `sub_10005E898`.

use crate::*;

impl RenderBridge {
    pub(crate) fn update_scene_bounce(&mut self, delta: f64) {
        // sub_10005E898 walks the ordered RenderObjectData map and updates the
        // collision squash/stretch entirely in float32. GameLua+0x368 is
        // initialized to zero and never written in Purple, so its guarded
        // phase divisor is always one.
        let delta = delta as f32;
        let threshold = (self.game_world_scale as f32) * 0.01_f32;
        for (index, object) in self.scene.values_mut().enumerate() {
            if !object.bounce_active {
                continue;
            }
            let elapsed = object.bounce_elapsed as f32 + delta;
            object.bounce_elapsed = f64::from(elapsed);
            let initial = object.bounce_initial_amplitude as f32;
            let decay = (-initial).mul_add(elapsed, initial);
            let amplitude = ((object.bounce_amplitude_multiplier as f32) * decay).min(4.0_f32);
            object.bounce_current_amplitude = f64::from(amplitude);
            if amplitude <= threshold {
                object.bounce_active = false;
                object.bounce_elapsed = 0.0;
                object.bounce_current_amplitude = 0.0;
                object.bounce_initial_amplitude = 0.0;
                continue;
            }

            let frequency = amplitude.mul_add(
                100.0_f32,
                (object.bounce_frequency_multiplier as f32) * 5.0_f32,
            );
            let phase = frequency.mul_add(elapsed, index as f32 * std::f32::consts::PI * 0.5_f32);
            object.scale_x = f64::from(amplitude.mul_add(phase.sin(), object.base_scale_x as f32));
            object.scale_y = f64::from(amplitude.mul_add(
                (std::f32::consts::PI + phase).sin(),
                object.base_scale_y as f32,
            ));
        }
    }

    pub(crate) fn advance_native_scene_frame(&mut self, delta: f64) -> (bool, bool, bool) {
        // sub_10005E898 performs this pass once per rendered frame after all
        // fixed Box2D steps. Its three Lua globals deliberately use two
        // different motion tolerances, and P_IGNORE_MOTION only affects the
        // moving aggregates -- it does not disable solving or sleeping.
        let delta = delta as f32;
        let mut has_moving_objects = false;
        let mut has_awake_objects = false;
        let mut has_moving_objects_zero_tolerance = false;
        for object in self.scene.values_mut() {
            if !object.has_physics_body() {
                continue;
            }

            // SceneObject+0x128 is incremented only for controllable objects
            // whose timer has been armed with a non-negative value.
            if object.controllable && object.time_since_collision >= 0.0 {
                object.time_since_collision =
                    f64::from((object.time_since_collision as f32) + delta);
            }

            // +0x145 caches b2Body::e_awakeFlag. The previous value keeps an
            // object in this reporting pass for the single frame in which it
            // transitions to sleeping.
            let awake = !object.sleeping;
            // sub_10005E898 skips a body with zero mass unless it is
            // kinematic. Static fixtures can carry Box2D's awake flag, but do
            // not contribute to any of the three Lua motion aggregates.
            let reports_motion = object.kinematic_body || object.inverse_mass > 0.0;
            if reports_motion && (object.native_was_awake || awake) {
                has_awake_objects = true;
                let velocity_x = object.velocity_x as f32;
                let velocity_y = object.velocity_y as f32;
                let linear_speed_squared = velocity_x.mul_add(velocity_x, velocity_y * velocity_y);
                if !object.ignore_motion {
                    if !object.controllable && linear_speed_squared > 0.0_f32 {
                        has_moving_objects_zero_tolerance = true;
                    }
                    if linear_speed_squared > 9.0_f32
                        || (object.angular_velocity as f32).abs() > 0.1_f32
                    {
                        has_moving_objects = true;
                    }
                }
            }
            object.native_was_awake = awake;
        }
        self.update_scene_bounce(f64::from(delta));
        (
            has_moving_objects,
            has_awake_objects,
            has_moving_objects_zero_tolerance,
        )
    }
}
