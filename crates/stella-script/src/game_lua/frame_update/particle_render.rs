//! Particle draw member `sub_100091D90` and its four GameLua entry modes.

use crate::*;

impl RenderBridge {
    pub(crate) fn draw_particles(&mut self, mode: i32) {
        let world_scale = self.world_scale as f32;
        let menu_particles_scale = self.particle_system.scale;
        let top_left_x = self.top_left_x as f32;
        let top_left_y = self.top_left_y as f32;
        let commands = self
            .particle_system
            .particles
            .iter()
            .filter(|particle| particle.mode == mode)
            .map(|particle| {
                let (bound_region, bound_composite) = particle.draw_bindings();
                // sub_100091D90 uses its world transform branch only for
                // modes 1/2. Modes 3/4 retain framebuffer coordinates and
                // multiply scale by Particles+0x38.
                let world_mode = matches!(mode, 1 | 2);
                let render_scale = if world_mode {
                    world_scale * particle.current_scale
                } else {
                    particle.current_scale * menu_particles_scale
                };
                let coordinate_divisor = if world_mode {
                    particle.current_scale
                } else {
                    render_scale
                };
                RenderCommand {
                    order: 0,
                    sprite: particle.sprite.clone(),
                    texture: None,
                    bound_region,
                    bound_composite,
                    geometry: None,
                    shader: None,
                    clip_holes: Vec::new(),
                    dirt: None,
                    // AtlasSprite/CompoSprite receives position divided by
                    // the same scale installed in the current GL state. The
                    // renderer then performs FADD followed by FMUL; retaining
                    // that representation avoids collapsing three float32
                    // boundaries into one host-double expression.
                    x: f64::from(particle.x / coordinate_divisor),
                    y: f64::from(particle.y / coordinate_divisor),
                    state: RenderState {
                        translate_x: if world_mode {
                            f64::from(-top_left_x / particle.current_scale)
                        } else {
                            0.0
                        },
                        translate_y: if world_mode {
                            f64::from(-top_left_y / particle.current_scale)
                        } else {
                            0.0
                        },
                        scale_x: f64::from(render_scale),
                        scale_y: f64::from(render_scale),
                        angle: f64::from(particle.angle),
                        alpha: 1.0,
                        ..RenderState::default()
                    },
                    world_space: false,
                }
            })
            .collect::<Vec<_>>();
        self.extend_render_commands(commands);
        // Unlike ThemeParticleSystem, ordinary Particles does not restore a
        // saved state. `sub_100091D90` copies a freshly constructed default
        // 0x9C record both before its loop and again before returning.
        self.state = RenderState::default();
    }
}
