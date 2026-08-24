//! ThemeManager world-relative layer refresh (`sub_10009A894`).

use crate::*;

impl RenderBridge {
    pub(super) fn refresh_theme_world_offsets(
        &mut self,
        foreground: bool,
        limits: ThemeWorldLimits,
    ) {
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
        }
    }
}
