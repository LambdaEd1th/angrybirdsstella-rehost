//! Native theme frame chain from `sub_10005E898`.

mod layers;
mod sprite_data;

use crate::*;

impl RenderBridge {
    /// The level loader's optional particle pre-roll advances only the two
    /// ThemeParticleSystem instances; layer animation/motion and nested
    /// ThemeSpriteData remain untouched.
    pub(crate) fn advance_theme_particles_only(
        &mut self,
        delta: f32,
        resources: &ResourceRuntime,
        data_root: &Path,
    ) {
        let background_indices = self
            .theme_background_layers
            .iter()
            .map(|layer| layer.definition_index as i32)
            .collect::<Vec<_>>();
        let foreground_indices = self
            .theme_foreground_layers
            .iter()
            .map(|layer| layer.definition_index as i32)
            .collect::<Vec<_>>();
        let random = &mut self.particle_random;
        let background_particles = &mut self.theme_background_particles;
        let foreground_particles = &mut self.theme_foreground_particles;
        for index in background_indices.into_iter().chain(foreground_indices) {
            let bindings = Some((resources, data_root));
            background_particles.update_layer(index, delta, random, bindings);
            foreground_particles.update_layer(index, delta, random, bindings);
        }
    }

    #[cfg(test)]
    pub(crate) fn advance_native_theme_frame(&mut self, delta: f64) {
        self.advance_native_theme_frame_inner(delta, None);
    }

    #[cfg(test)]
    pub(crate) fn advance_native_theme_frame_with_limits(
        &mut self,
        delta: f64,
        world_limits: ThemeWorldLimits,
    ) {
        self.theme_camera.world_limits = world_limits;
        self.advance_native_theme_frame_inner(delta, None);
    }

    pub(crate) fn advance_native_theme_frame_with_resources(
        &mut self,
        delta: f64,
        resources: &ResourceRuntime,
        data_root: &Path,
    ) {
        self.advance_native_theme_frame_inner(delta, Some((resources, data_root)));
    }

    fn advance_native_theme_frame_inner(
        &mut self,
        delta: f64,
        bindings: Option<(&ResourceRuntime, &Path)>,
    ) {
        if self.accelerometer_active {
            // 0x10005EC9C..0x10005ECD8 widens each raw float sample, applies
            // the exact double 0.2 coefficient, narrows, and only then uses
            // float32 FMADD for previous*0.8 + sampleTerm.
            for index in 0..2 {
                let sample_term = (f64::from(self.accelerometer_sample[index]) * 0.2_f64) as f32;
                self.accelerometer_filtered[index] =
                    self.accelerometer_filtered[index].mul_add(0.8_f32, sample_term);
            }
        }
        // 0x10005ED04 / 0x10005ED1C select the ThemeManager mode before
        // invoking the same member for the two native layer vectors.
        layers::advance_theme_manager_pass(self, false, delta as f32, bindings);
        layers::advance_theme_manager_pass(self, true, delta as f32, bindings);
        // 0x10005ED28 then advances GameLua's layer positions and nested
        // 136-byte ThemeSpriteData vectors.
        sprite_data::advance_game_lua_theme_data(self, delta as f32);
    }
}
