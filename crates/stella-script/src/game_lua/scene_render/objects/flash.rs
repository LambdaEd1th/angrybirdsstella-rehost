//! Flash-animation transform used by the separate `sub_10006794C` member.

use super::SceneDrawObject;
use crate::*;

impl RenderBridge {
    pub(crate) fn flash_animation_transform(&self, object: &SceneDrawObject) -> AnimationTransform {
        // sub_10006794C does not apply gameWorldScale; a horizontal flip
        // changes only animation X scale, not rotation.
        let horizontal_sign = if object.horizontal_flip {
            -1.0_f32
        } else {
            1.0_f32
        };
        let world_scale = self.world_scale as f32;
        // Purple uses FNMSUB for (position / 0.05 - camera), then FMUL
        // by world scale. Keep the fused float32 subtraction and the order
        // of the two X-scale multiplies visible in the native callback.
        AnimationTransform {
            x: f64::from((object.x as f32).mul_add(20.0, -(self.top_left_x as f32)) * world_scale),
            y: f64::from((object.y as f32).mul_add(20.0, -(self.top_left_y as f32)) * world_scale),
            scale_x: f64::from((horizontal_sign * world_scale) * object.scale_x as f32),
            scale_y: f64::from(world_scale * object.scale_y as f32),
            // +0xAC already contains native interpolation or Lua's explicit
            // setRotation. No action-specific velocity writer follows it.
            angle: f64::from(object.angle as f32),
        }
    }
}
