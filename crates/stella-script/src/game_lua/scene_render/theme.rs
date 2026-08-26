//! Theme-layer draw and repeat traversal (`sub_10009BDB4`).

use crate::*;

mod position;
mod repeat;
mod world_offsets;

impl RenderBridge {
    pub(crate) fn draw_theme_pass(
        &mut self,
        foreground: bool,
        selected_layer: Option<usize>,
        world_limits: ThemeWorldLimits,
        resources: &ResourceRuntime,
        data_root: &Path,
    ) {
        if self.game_rendering_disabled {
            return;
        }
        if !foreground {
            // `0x10009BE6C..0x10009BE88` applies GameLua's stored sky color
            // only for ThemeManager mode one. This deliberately happens at
            // background draw time rather than inside `setTheme`.
            self.background_color = self.theme_sky_color.map(native_theme_color_channel);
        }
        self.refresh_theme_world_offsets(foreground, world_limits);
        let layer_count = if foreground {
            self.theme_foreground_layers.len()
        } else {
            self.theme_background_layers.len()
        };
        let (first_layer, end_layer) = match selected_layer {
            Some(index) if index < layer_count => (index, index + 1),
            Some(_) => return,
            None => (0, layer_count),
        };
        for index in first_layer..end_layer {
            // Purple traverses its retained ThemeLayer vector in place. Build
            // only the small immutable draw snapshot needed after this borrow
            // ends; cloning the complete layer here used to duplicate every
            // animation-frame string/timeline once per pass and frame.
            let (sprite, geometry, definition_index, alpha, transform, positions) = {
                let layer = if foreground {
                    &self.theme_foreground_layers[index]
                } else {
                    &self.theme_background_layers[index]
                };
                let transform = position::layer_transform(self, layer, foreground);
                let positions = repeat::tile_positions(self, layer, &transform);
                (
                    layer.sprite.clone(),
                    layer.geometry,
                    layer.definition_index,
                    layer.alpha,
                    transform,
                    positions,
                )
            };
            self.draw_theme_particles_for_layer(foreground, definition_index as i32, &transform);

            let bound_region = resources
                .active_atlas_catalog_region(&sprite, data_root)
                .map(Arc::new);
            let bound_composite = resources.active_bound_composite(&sprite).map(Arc::new);
            if bound_region.is_none() && bound_composite.is_none() {
                continue;
            }
            let sprite: SharedSpriteName = sprite.into();
            let scale_x = transform.scale_x;
            let scale_y = transform.scale_y;
            // ThemeManager calls ResourceManager::drawSprite with both anchor
            // arguments set to CENTER (0x10009C16C/0x10009C1B0). AtlasSprite
            // then submits its raw rectangle, while CompositeSprite applies
            // the center offset to its part coordinate system. Keep the
            // repeated x/y as the native geometric center used for culling,
            // and carry the corresponding local displacement in render state.
            let (anchor_x, anchor_y) = sprite_draw_anchor_offset_from_geometry(
                geometry,
                SpriteHorizontalAnchor::Center,
                SpriteVerticalAnchor::Center,
            );
            let is_atlas_sprite = bound_region.is_some();
            let (local_anchor_x, local_anchor_y, sprite_pivot) = if is_atlas_sprite {
                (
                    anchor_x + geometry.min_x,
                    anchor_y + geometry.min_y,
                    Some([0.0, 0.0]),
                )
            } else {
                (anchor_x, anchor_y, None)
            };
            let command = |x, y| RenderCommand {
                order: 0,
                sprite: sprite.clone(),
                texture: None,
                texture_scale: 1.0,
                masked_texture_binding: None,
                bound_region: bound_region.clone(),
                bound_composite: bound_composite.clone(),
                geometry: None,
                shader: None,
                clip_holes: Vec::new(),
                dirt: None,
                x,
                y,
                state: RenderState {
                    translate_x: local_anchor_x * scale_x,
                    translate_y: local_anchor_y * scale_y,
                    scale_x,
                    scale_y,
                    alpha,
                    sprite_pivot,
                    ..RenderState::default()
                },
                world_space: true,
            };
            let layer_commands = positions.into_iter().map(|(x, y)| command(x, y));
            self.extend_render_commands(layer_commands);
        }
    }

    /// `sub_100096E9C`, called at `0x10009C17C/0x10009C1A8` before the
    /// corresponding layer image is submitted. Particle coordinates are
    /// local to the layer's world-space origin, while their scale is the
    /// normal camera scale rather than the layer sprite's parallax scale.
    fn draw_theme_particles_for_layer(
        &mut self,
        foreground: bool,
        layer_index: i32,
        transform: &position::ThemeLayerTransform,
    ) {
        let particles = if foreground {
            self.theme_foreground_particles.particles.get(&layer_index)
        } else {
            self.theme_background_particles.particles.get(&layer_index)
        };
        let Some(particles) = particles else {
            return;
        };

        let inherited_state = self.state;
        let world_scale = self.world_scale as f32;
        if world_scale == 0.0 || !world_scale.is_finite() {
            return;
        }
        let [layer_x, layer_y] = transform.native_world.unwrap_or([
            self.top_left_x as f32 + transform.x as f32 / world_scale,
            self.top_left_y as f32 + transform.y as f32 / world_scale,
        ]);
        let top_left_x = self.top_left_x as f32;
        let top_left_y = self.top_left_y as f32;
        let commands = particles
            .iter()
            .map(|particle| {
                let (bound_region, bound_composite) = particle.draw_bindings();
                let render_scale = world_scale * particle.current_scale;
                RenderCommand {
                    order: 0,
                    sprite: particle.sprite.clone(),
                    texture: None,
                    texture_scale: 1.0,
                    masked_texture_binding: None,
                    bound_region,
                    bound_composite,
                    geometry: None,
                    shader: None,
                    clip_holes: Vec::new(),
                    dirt: None,
                    x: f64::from((particle.x + layer_x) / particle.current_scale),
                    y: f64::from((particle.y + layer_y) / particle.current_scale),
                    state: RenderState {
                        translate_x: f64::from(-top_left_x / particle.current_scale),
                        translate_y: f64::from(-top_left_y / particle.current_scale),
                        scale_x: f64::from(render_scale),
                        scale_y: f64::from(render_scale),
                        angle: f64::from(particle.angle),
                        matrix: None,
                        sprite_pivot: None,
                        draw_size: None,
                        ..inherited_state
                    },
                    world_space: false,
                }
            })
            .collect::<Vec<_>>();
        self.extend_render_commands(commands);
    }
}

fn native_theme_color_channel(value: f32) -> u8 {
    let value = value.max(0.0);
    if value > 255.0 { 255 } else { value as u8 }
}
