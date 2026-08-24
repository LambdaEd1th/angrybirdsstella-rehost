//! GameLua layer positions and nested ThemeSpriteData (`sub_1000607E8`).

use crate::*;

pub(super) fn advance_game_lua_theme_data(bridge: &mut RenderBridge, delta: f32) {
    // Each 0x130-byte layer record first integrates +0x2c/+0x30 into the
    // independent posX/posY pair at +0x34/+0x38.
    for layer in bridge
        .theme_background_layers
        .iter_mut()
        .chain(bridge.theme_foreground_layers.iter_mut())
    {
        layer.position_x =
            f64::from((layer.velocity_x as f32).mul_add(delta, layer.position_x as f32));
        layer.position_y =
            f64::from((layer.velocity_y as f32).mul_add(delta, layer.position_y as f32));
    }

    let background_count = bridge.theme_background_layers.len();
    let physics_scale = bridge.physics_simulation_scale as f32;
    let screen_width = bridge.screen_width as f32;
    let screen_height = bridge.screen_height as f32;
    let horizontal_limit = 4.0 * screen_width / physics_scale;
    let horizontal_reset = 2.5 * screen_width / physics_scale;
    let vertical_limit = screen_height / physics_scale;
    let mut expired = Vec::new();

    for (entry_index, (key, sprite)) in bridge.theme_sprites.iter_mut().enumerate() {
        sprite.x = f64::from((sprite.velocity_x as f32).mul_add(delta, sprite.x as f32));
        sprite.y = f64::from((sprite.velocity_y as f32).mul_add(delta, sprite.y as f32));

        // The foreground vector is the second native loop and wraps relative
        // to ThemeSpriteData+0x7c/+0x80. Background entries never wrap.
        if key.0 >= background_count {
            if ((sprite.x as f32) - (sprite.original_x as f32)).abs() > horizontal_limit {
                sprite.x = f64::from(if (sprite.velocity_x as f32) > 0.0 {
                    (sprite.original_x as f32) - horizontal_reset
                } else {
                    (sprite.original_x as f32) + horizontal_reset
                });
            }
            if ((sprite.y as f32) - (sprite.original_y as f32)).abs() > vertical_limit {
                sprite.y = f64::from(if (sprite.velocity_y as f32) > 0.0 {
                    (sprite.original_y as f32) - vertical_limit
                } else {
                    (sprite.original_y as f32) + vertical_limit
                });
            }
        }

        let scale_delta = (sprite.scale_speed as f32) * delta;
        let scale_x = (sprite.scale_x as f32) + scale_delta;
        if scale_x > 0.0 {
            sprite.scale_x = f64::from(scale_x);
            sprite.scale_y = f64::from((sprite.scale_y as f32) + scale_delta);
        }

        if key.0 < background_count && sprite.is_animation && !sprite.animation_frames.is_empty() {
            if sprite.animation_timer <= 0.0 {
                sprite.animation_timer = sprite.animation_frame_time;
                sprite.animation_frame += 1;
                if sprite.animation_frame >= sprite.animation_frames.len() {
                    if sprite.animation_looping {
                        sprite.animation_frame = 0;
                    } else {
                        expired.push(entry_index);
                        continue;
                    }
                }
                sprite.sprite = sprite.animation_frames[sprite.animation_frame].clone();
            } else {
                sprite.animation_timer = f64::from((sprite.animation_timer as f32) - delta);
            }
        }
    }
    for index in expired.into_iter().rev() {
        bridge.theme_sprites.remove_index(index);
    }
}
