//! Render state reconstructed by the ordinary object member `sub_10006D5B4`.

use super::pivot::native_scene_callback_pivot;
use super::{SceneCallbackObject, SceneDrawObject, SceneDrawVisit};
use crate::*;

impl RenderBridge {
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
        let mut translate_x = ((object.x as f32 * 20.0_f32) - self.top_left_x as f32) * world_scale;
        let mut translate_y = ((object.y as f32 * 20.0_f32) - self.top_left_y as f32) * world_scale;
        let textured = object.masked_texture().is_some();
        // The alpha-masked branch at 0x10004C14C does not enter
        // sub_10006D5B4. It uses the raw +0xBC/+0xC0 scales and the caller's
        // already-composed +0xAC/+0xB0 angle instead. Ordinary sprites keep
        // the separate body/flip reconstruction below.
        let (scale_x, scale_y, angle) = if textured {
            (
                world_scale * object.scale_x as f32,
                world_scale * object.scale_y as f32,
                object.angle as f32 + object.sprite_rotation as f32,
            )
        } else {
            (
                (horizontal_sign * world_scale) * object_scale_x,
                world_scale * object_scale_y,
                object.angle as f32,
            )
        };
        let masked_texture_matrix = textured.then(|| {
            // The atlas pivot is already subtracted by the host region. The
            // live GL-context pivot is atlasPivot + RenderObjectData's two
            // offsets, leaving only those offsets relative to local vertices.
            RenderState::native_masked_texture_matrix(
                object.x as f32 * 20.0_f32,
                object.y as f32 * 20.0_f32,
                object.scale_x as f32,
                object.scale_y as f32,
                angle,
                object.pivot_offset_x as f32,
                object.pivot_offset_y as f32,
            )
        });
        let matrix = masked_texture_matrix.map(|[tx, ty, m00, m01, m10, m11]| {
            // sub_10008D428 rotates first and applies each context axis scale
            // afterward. Its screen origin also adds the live pivot offset;
            // the fill phase above deliberately omits that screen correction.
            translate_x = ((tx as f32 + object.scale_x as f32 * object.pivot_offset_x as f32)
                - self.top_left_x as f32)
                * world_scale;
            translate_y = ((ty as f32 + object.scale_y as f32 * object.pivot_offset_y as f32)
                - self.top_left_y as f32)
                * world_scale;
            [m00, m01, m10, m11].map(|v| f64::from(v as f32 * world_scale))
        });
        RenderState {
            custom_model: self.state.custom_model,
            translate_x: f64::from(translate_x),
            translate_y: f64::from(translate_y),
            scale_x: f64::from(scale_x),
            scale_y: f64::from(scale_y),
            // sub_10006C838 builds T * R * Scale. Flip is Scale.x's sign.
            // Its ordinary-sprite call receives RenderObjectData+0xAC only;
            // +0xB0 (`setSpriteRotation`) belongs to the surrounding live
            // callback context and is not added to this explicit matrix.
            angle: f64::from(angle),
            matrix,
            masked_texture_matrix,
            sprite_pivot: None,
            pivot_x: 0.0,
            pivot_y: 0.0,
            draw_size: None,
            alpha: object.alpha,
            clip_rect: self.state.clip_rect,
        }
    }

    /// GL context left by the object branch immediately before +0x160 post.
    /// The +0x158 pre callback runs before this state is installed.
    #[cfg(test)]
    pub(crate) fn scene_post_draw_state(&self, object: &SceneDrawObject) -> RenderState {
        self.native_scene_post_draw_state(object)
    }

    fn native_scene_post_draw_state(&self, object: &SceneDrawObject) -> RenderState {
        let (pivot_x, pivot_y) = native_scene_callback_pivot(
            object.composite_sprite.as_deref().map(Vec::as_slice),
            object.sprite_region.as_deref(),
            object.pivot_offset_x,
            object.pivot_offset_y,
        );
        let world_scale = self.world_scale as f32;
        let object_angle = object.angle as f32;
        let composed_angle = object_angle + object.sprite_rotation as f32;
        let clip_rect = self.state.clip_rect;

        // The custom/ray branch leaves the state installed by
        // 0x10004BFE0..0x10004C104: camera translation and world scale are
        // still separate from the object transform passed to the callback.
        if object.flash_animation || object.ray.is_some() {
            return RenderState {
                custom_model: self.state.custom_model,
                translate_x: f64::from(-(self.top_left_x as f32)),
                translate_y: f64::from(-(self.top_left_y as f32)),
                scale_x: f64::from(world_scale),
                scale_y: f64::from(world_scale),
                angle: f64::from(composed_angle),
                matrix: None,
                masked_texture_matrix: None,
                sprite_pivot: None,
                pivot_x: f64::from(pivot_x),
                pivot_y: f64::from(pivot_y),
                draw_size: None,
                alpha: object.alpha,
                clip_rect,
            };
        }

        // The masked-texture branch at 0x10004C14C bypasses sub_10006D5B4
        // and divides only the camera translation by raw +0xBC/+0xC0.
        if object.masked_texture().is_some() {
            let scale_x = object.scale_x as f32;
            let scale_y = object.scale_y as f32;
            return RenderState {
                custom_model: self.state.custom_model,
                translate_x: f64::from(-(self.top_left_x as f32) / scale_x),
                translate_y: f64::from(-(self.top_left_y as f32) / scale_y),
                scale_x: f64::from(world_scale * scale_x),
                scale_y: f64::from(world_scale * scale_y),
                angle: f64::from(composed_angle),
                matrix: None,
                masked_texture_matrix: None,
                sprite_pivot: None,
                pivot_x: f64::from(pivot_x),
                pivot_y: f64::from(pivot_y),
                draw_size: None,
                alpha: object.alpha,
                clip_rect,
            };
        }

        // Ordinary/composite rendering enters sub_10006D5B4, which applies
        // body scale, flip and the secondary pivot-offset translation in
        // place. These values remain live for the post callback.
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
        let object_scale_x = object.scale_x as f32 * body_scale;
        let object_scale_y = object.scale_y as f32 * body_scale;
        let horizontally_flipped = horizontal_sign < 0.0;
        let translate_x = (horizontal_sign
            * (object.pivot_offset_x as f32 - self.top_left_x as f32))
            / object_scale_x;
        let translate_y = (object.pivot_offset_y as f32 - self.top_left_y as f32) / object_scale_y;
        let scale_x = (horizontal_sign * world_scale) * object_scale_x;
        let scale_y = world_scale * object_scale_y;
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
            custom_model: self.state.custom_model,
            translate_x: f64::from(translate_x),
            translate_y: f64::from(translate_y),
            scale_x: f64::from(scale_x),
            scale_y: f64::from(scale_y),
            angle: f64::from(callback_angle),
            matrix: None,
            masked_texture_matrix: None,
            sprite_pivot: None,
            pivot_x: f64::from(pivot_x),
            pivot_y: f64::from(pivot_y),
            draw_size: None,
            alpha: object.alpha,
            clip_rect,
        }
    }

    pub(crate) fn scene_draw_visit(&self, name: &str) -> Option<SceneDrawVisit> {
        if self.orphaned_native_bodies.contains(name) {
            return None;
        }
        let object = self.scene.get(name)?;
        if !object.visible {
            return None;
        }
        Some(SceneDrawVisit {
            callback_slot: object.draw_callback_slot,
            callback: SceneCallbackObject::from(object),
            initial_draw_object: SceneDrawObject::from(object),
        })
    }

    pub(crate) fn install_scene_post_draw_state(&mut self, object: &SceneDrawObject) {
        self.state = self.native_scene_post_draw_state(object);
    }
}
