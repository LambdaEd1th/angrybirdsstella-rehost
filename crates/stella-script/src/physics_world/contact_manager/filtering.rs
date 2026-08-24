//! GameLua/Box2D collision filtering.

use crate::*;

impl RenderBridge {
    /// GameLua's Box2D contact filter at sub_100065488. Collision groups are
    /// game-side exclusion groups (equal positive ids suppress a pair), not
    /// Box2D's usual positive groupIndex override.
    pub(crate) fn native_objects_should_collide(first: &SceneObject, second: &SceneObject) -> bool {
        if first.collision_group == second.collision_group && first.collision_group > 0 {
            return false;
        }
        if !first.collision_enabled || !second.collision_enabled {
            return false;
        }

        let first_type = first.sensor_type;
        let second_type = second.sensor_type;
        if (first_type == 9 || second_type == 9)
            && (!first.dynamic_body || !second.dynamic_body)
            && first.dirt.is_none()
            && second.dirt.is_none()
        {
            return false;
        }
        if (first_type == 7 && second.controllable) || (second_type == 7 && first.controllable) {
            return false;
        }
        if (first_type & !2) == 5 && (second_type & !2) == 5 {
            return false;
        }
        if (first_type == 6 || second_type == 6) && (first.controllable || second.controllable) {
            return false;
        }
        if !first.controllable
            && !second.controllable
            && first_type != 6
            && second_type != 6
            && ((first_type == 5) != (second_type == 5))
        {
            return false;
        }

        if first.block_collision_enabled {
            if second.block_collision_enabled {
                true
            } else {
                second
                    .collision_materials
                    .iter()
                    .any(|material| material == &first.material)
            }
        } else {
            first
                .collision_materials
                .iter()
                .any(|material| material == &second.material)
        }
    }
}
