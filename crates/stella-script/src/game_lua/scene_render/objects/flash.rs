//! Flash-animation transform used by the separate `sub_10006794C` member.

use super::SceneDrawObject;
use crate::*;

impl RenderBridge {
    pub(crate) fn flash_animation_transform(
        &self,
        object: &SceneDrawObject,
        current_action: &str,
    ) -> AnimationTransform {
        // sub_10006794C does not apply gameWorldScale; a horizontal flip
        // changes only animation X scale, not rotation.
        let horizontal_sign = if object.horizontal_flip { -1.0 } else { 1.0 };
        AnimationTransform {
            x: (object.x * 20.0 - self.top_left_x) * self.world_scale,
            y: (object.y * 20.0 - self.top_left_y) * self.world_scale,
            scale_x: horizontal_sign * object.scale_x * self.world_scale,
            scale_y: object.scale_y * self.world_scale,
            angle: self
                .interpolated_flying_bird_angle(object, current_action)
                .unwrap_or(object.angle),
        }
    }
}
