//! Flash-animation transform used by the separate `sub_10006794C` member.

use super::SceneDrawObject;
use crate::*;

impl RenderBridge {
    pub(crate) fn flash_animation_transform(
        &self,
        object: &SceneDrawObject,
        sample_fixed_step_motion: bool,
        sample_stella_flight_angle: bool,
    ) -> AnimationTransform {
        // sub_10006794C does not apply gameWorldScale; a horizontal flip
        // changes only animation X scale, not rotation.
        let horizontal_sign = if object.horizontal_flip { -1.0 } else { 1.0 };
        let (x, y) = if sample_fixed_step_motion {
            self.fixed_step_display_position(object)
        } else {
            (object.x, object.y)
        };
        AnimationTransform {
            x: (x * 20.0 - self.top_left_x) * self.world_scale,
            y: (y * 20.0 - self.top_left_y) * self.world_scale,
            scale_x: horizontal_sign * object.scale_x * self.world_scale,
            scale_y: object.scale_y * self.world_scale,
            angle: if sample_stella_flight_angle {
                self.fixed_step_stella_flight_angle(object)
            } else {
                object.angle
            },
        }
    }
}
