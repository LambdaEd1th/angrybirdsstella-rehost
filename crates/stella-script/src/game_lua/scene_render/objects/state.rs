//! Render state reconstructed by the ordinary object member `sub_10006D5B4`.

use super::SceneDrawObject;
use crate::*;

impl RenderBridge {
    /// Produce a continuous display pose for a Flash action whose root is
    /// driven by the fixed-step physics body.
    ///
    /// Purple advances Box2D in fixed 1/30-second game-time steps. Poppy's
    /// aiming slow motion lowers the game-time multiplier to 0.1, so binding
    /// her Flash root directly to the last solved body pose repeats it for
    /// roughly twenty 60 Hz display frames; Luca's 0.05 multiplier repeats it
    /// for roughly forty. Stella's timeout path is initially a smooth
    /// real-time `setPosition` tween, but switches back to the same 30 Hz body
    /// when `Stella_Flying` resumes. The unsolved accumulator is already the
    /// exact game-time distance to the next fixed step; advancing only the
    /// visual pose by the current velocity fills those display samples
    /// without changing the body, Lua object or collision timeline.
    pub(crate) fn fixed_step_display_position(&self, object: &SceneDrawObject) -> (f64, f64) {
        let residual = self
            .physics_accumulator
            .clamp(0.0, f32::from_bits(0x3D08_8889));
        let x = (object.velocity_x as f32).mul_add(residual, object.x as f32);
        let y = (object.velocity_y as f32).mul_add(residual, object.y as f32);
        (f64::from(x), f64::from(y))
    }

    /// Predict the high-speed direction used by BirdAnimation's
    /// `Stella_Flying` state at the same unsolved display time.
    pub(crate) fn fixed_step_stella_flight_angle(&self, object: &SceneDrawObject) -> f64 {
        let residual = self
            .physics_accumulator
            .clamp(0.0, f32::from_bits(0x3D08_8889));
        let gravity_scale = object.gravity_scale as f32;
        let velocity_x = ((self.world_gravity_x as f32) * gravity_scale)
            .mul_add(residual, object.velocity_x as f32);
        let velocity_y = ((self.world_gravity_y as f32) * gravity_scale)
            .mul_add(residual, object.velocity_y as f32);
        let speed_squared = velocity_x.mul_add(velocity_x, velocity_y * velocity_y);
        if speed_squared > 4.0_f32 {
            f64::from(velocity_y.atan2(velocity_x))
        } else {
            object.angle
        }
    }

    pub(crate) fn scene_object_scale(&self, object: &SceneDrawObject) -> (f32, f32, f32) {
        // Game world scale applies only when +0x100 truncates to 2 and the
        // circle-constructor byte at +0x147 is clear.
        let body_scale = if object.sensor_type == 2 && !object.collision_is_circle {
            self.game_world_scale as f32
        } else {
            1.0_f32
        };
        let horizontal_sign = if object.horizontal_flip {
            -1.0_f32
        } else {
            1.0_f32
        };
        (
            horizontal_sign,
            object.scale_x as f32 * body_scale,
            object.scale_y as f32 * body_scale,
        )
    }

    pub(crate) fn scene_object_state(&self, object: &SceneDrawObject) -> RenderState {
        let (horizontal_sign, object_scale_x, object_scale_y) = self.scene_object_scale(object);
        let world_scale = self.world_scale as f32;
        // sub_10006D5B4 first rounds the body position * 20 in S0/S1. Its
        // sub_10006C838 callee then executes FSUB followed by FMUL for the
        // screen origin, and (flip * worldScale) followed by FMUL with the
        // object scale for the linear basis. Preserve those float32 stage
        // boundaries instead of collapsing the expression in host f64.
        let translate_x = ((object.x as f32 * 20.0_f32) - self.top_left_x as f32) * world_scale;
        let translate_y = ((object.y as f32 * 20.0_f32) - self.top_left_y as f32) * world_scale;
        let scale_x = (horizontal_sign * world_scale) * object_scale_x;
        let scale_y = world_scale * object_scale_y;
        RenderState {
            translate_x: f64::from(translate_x),
            translate_y: f64::from(translate_y),
            scale_x: f64::from(scale_x),
            scale_y: f64::from(scale_y),
            // sub_10006C838 builds T * R * Scale. Flip is Scale.x's sign.
            // Its ordinary-sprite call receives RenderObjectData+0xAC only;
            // +0xB0 (`setSpriteRotation`) belongs to the surrounding live
            // callback context and is not added to this explicit matrix.
            angle: f64::from(object.angle as f32),
            matrix: None,
            sprite_pivot: None,
            pivot_x: 0.0,
            pivot_y: 0.0,
            draw_size: None,
            explicit_quad: None,
            native_sprite_quad: None,
            alpha: object.alpha,
            clip_rect: self.state.clip_rect,
        }
    }

    pub(crate) fn scene_callback_state(&self, object: &SceneDrawObject) -> RenderState {
        let (horizontal_sign, object_scale_x, object_scale_y) = self.scene_object_scale(object);
        let horizontally_flipped = horizontal_sign < 0.0;
        let world_scale = self.world_scale as f32;
        let translate_x = (horizontal_sign
            * (object.pivot_offset_x as f32 - self.top_left_x as f32))
            / object_scale_x;
        let translate_y = (object.pivot_offset_y as f32 - self.top_left_y as f32) / object_scale_y;
        let scale_x = (horizontal_sign * world_scale) * object_scale_x;
        let scale_y = world_scale * object_scale_y;
        let object_angle = object.angle as f32;
        let (pivot_x, pivot_y) = object.callback_pivot();
        let callback_angle = if horizontally_flipped {
            // The flip branch at 0x10006D640 overwrites the live angle with
            // FNEG of +0xAC and deliberately drops +0xB0.
            -object_angle
        } else {
            // Without a flip, sub_10006D5B4 retains the angle installed by
            // its caller at 0x10004C0E8: +0xAC plus +0xB0 in one FADD.
            object_angle + object.sprite_rotation as f32
        };
        // sub_10006D5B4 divides the secondary translation by object scale so
        // the camera contribution remains scale-independent.
        RenderState {
            translate_x: f64::from(translate_x),
            translate_y: f64::from(translate_y),
            scale_x: f64::from(scale_x),
            scale_y: f64::from(scale_y),
            angle: f64::from(callback_angle),
            matrix: None,
            sprite_pivot: None,
            pivot_x: f64::from(pivot_x),
            pivot_y: f64::from(pivot_y),
            draw_size: None,
            explicit_quad: None,
            native_sprite_quad: None,
            alpha: object.alpha,
            clip_rect: self.state.clip_rect,
        }
    }

    pub(crate) fn begin_scene_object_draw(&mut self, object: &SceneDrawObject) -> RenderState {
        let previous = self.state;
        self.state = self.scene_callback_state(object);
        previous
    }

    pub(crate) fn finish_scene_object_draw(&mut self, previous: RenderState) {
        self.state = previous;
    }
}
