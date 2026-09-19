//! ThemeManager world-relative layer refresh (`sub_10009A894`).

use crate::*;

impl RenderBridge {
    pub(crate) fn initialize_theme_world_offsets(&mut self, foreground: bool) {
        let limits = self.theme_camera.world_limits;
        let context = ThemeWorldOffsetContext {
            current_scale: self.world_scale as f32,
            screen_left: self.top_left_x as f32,
            screen_top: self.top_left_y as f32,
            screen_width: self.screen_width as f32,
            screen_height: self.screen_height as f32,
            reference_x: self.theme_camera.x,
            reference_y: self.theme_camera.y,
        };

        let layers = if foreground {
            &mut self.theme_foreground_layers
        } else {
            &mut self.theme_background_layers
        };
        for layer in layers {
            refresh_theme_layer_world_offset(layer, limits, context, &mut self.particle_random);
            if let Some(relative_y) = layer.relative_y {
                let offset = native_theme_relative_y_offset(
                    relative_y as f32,
                    self.screen_height as f32,
                    self.theme_camera.original_scale_ratio,
                );
                crate::game_lua::theme_world_offsets::set_native_offset_y(layer, offset);
            }
        }
    }
}
