//! `ThemeParticleSystem` (`sub_100096A48..sub_1000974D8`).

use std::cmp::Ordering;

use crate::*;

use super::{ParticleEmitter, emit_particles, prepare_particle_emitter};

#[derive(Debug, Clone)]
pub(crate) struct ThemeParticleSpawner {
    /// Spawner+0x18. A negative interval disables automatic emission while
    /// retaining the copied query for the native force-spawn members.
    pub(crate) interval: f32,
    /// Spawner+0x28 starts at zero, so the first positive-interval update
    /// emits immediately before resetting it to `interval`.
    pub(crate) timer: f32,
    emitter: ParticleEmitter,
}

/// The derived ThemeParticleSystem owns two red-black trees in addition to
/// the normal Particles definition cache: Spawners at +0x88 and vectors of
/// 0x68-byte ParticleData records at +0xB8.
#[derive(Debug, Default)]
pub(crate) struct NativeThemeParticles {
    pub(crate) definitions: BTreeMap<String, ParticleDefinition>,
    pub(crate) spawners: BTreeMap<i32, ThemeParticleSpawner>,
    pub(crate) particles: BTreeMap<i32, Vec<Particle>>,
}

impl NativeThemeParticles {
    /// `sub_100096B74`, fed by ThemeManager's `sub_100099168` layer walk.
    /// Insertion by source definition index replaces an earlier Spawner,
    /// which is observable for spawn-expanded theme records.
    pub(crate) fn install_layer_spawner(
        &mut self,
        lua: &Lua,
        layer: &ThemeLayer,
        mode: i32,
    ) -> LuaResult<()> {
        let Some(definition_name) = layer.particles.as_deref() else {
            return Ok(());
        };
        let query = lua.create_table()?;
        query.set("x", 0.0_f32)?;
        query.set("y", 0.0_f32)?;
        query.set("mode", mode)?;
        query.set("w", 0.0_f32)?;
        query.set("h", 0.0_f32)?;
        query.set("angle", 0.0_f32)?;
        query.set("amount", 0.0_f32)?;
        query.set("definitionName", definition_name)?;
        query.set("z", layer.z_distance as f32)?;
        query.set("themeLayerIndex", layer.definition_index as f32)?;
        let emitter = prepare_particle_emitter(lua, &mut self.definitions, &query)?;
        self.spawners.insert(
            layer.definition_index as i32,
            ThemeParticleSpawner {
                interval: layer.spawn_interval,
                timer: 0.0,
                emitter,
            },
        );
        Ok(())
    }

    /// `sub_100097090`. Theme particles use `(1-z)` for both acceleration
    /// and displacement, then apply that same parallax factor only to the
    /// begin/end scale interpolation term.
    pub(crate) fn update_layer(
        &mut self,
        layer_index: i32,
        delta: f32,
        random: &mut NativeParticleRandom,
        bindings: Option<(&ResourceRuntime, &Path)>,
    ) {
        let particles = self.particles.entry(layer_index).or_default();
        if let Some(spawner) = self.spawners.get_mut(&layer_index)
            // sub_100097168 uses FCMP/B.LT, so unordered (NaN) intervals
            // enter the timed path rather than behaving like a negative
            // disabled sentinel.
            && spawner.interval.partial_cmp(&0.0) != Some(Ordering::Less)
        {
            spawner.timer -= delta;
            // 0x100097180 uses B.GT to skip emission. An unordered timer
            // therefore emits once and is reset to the authored interval.
            if spawner.timer.partial_cmp(&0.0) != Some(Ordering::Greater) {
                // sub_10008E524 applies its soft/hard limits against the base
                // vector at Particles+0x40. ThemeParticleSystem's virtual
                // append member routes every record into the per-layer tree
                // at +0xB8, leaving that base count at zero.
                emit_particles(particles, random, &spawner.emitter, 0, bindings);
                spawner.timer = spawner.interval;
            }
        }
        particles.retain_mut(|particle| {
            debug_assert_eq!(particle.theme_layer_index, layer_index);
            particle.elapsed += delta;
            if particle.elapsed > particle.lifetime && particle.lifetime != -1.0_f32 {
                return false;
            }

            let parallax = 1.0_f32 - particle.z;
            particle.velocity_x =
                (particle.gravity_x * delta).mul_add(parallax, particle.velocity_x);
            particle.velocity_y =
                (particle.gravity_y * delta).mul_add(parallax, particle.velocity_y);
            particle.x = (particle.velocity_x * delta).mul_add(parallax, particle.x);
            particle.y = (particle.velocity_y * delta).mul_add(parallax, particle.y);
            particle.angle = particle.angular_velocity.mul_add(delta, particle.angle);
            let progress = particle.elapsed / particle.lifetime;
            let scale_delta = (particle.scale_end - particle.scale_begin) * progress;
            particle.current_scale = scale_delta.mul_add(parallax, particle.scale_begin);

            if particle.animate_over_lifetime && !particle.sprites.is_empty() {
                let count = particle.sprites.len();
                let mut frame = (progress * count as f32).ceil() as usize;
                if frame == 0 {
                    frame = 1;
                }
                frame = frame.min(count);
                if frame != particle.animation_frame {
                    particle.sprite = particle.sprites[frame - 1].as_str().into();
                    particle.animation_frame = frame;
                    if let Some((resources, data_root)) = bindings {
                        particle.bind_sprite(resources, data_root);
                    }
                }
            }
            true
        });
    }

    /// `sub_1000974D8`, used by the two ThemeManager force-spawn helpers.
    pub(crate) fn force_spawn_layer(
        &mut self,
        layer_index: i32,
        random: &mut NativeParticleRandom,
        resources: &ResourceRuntime,
        data_root: &Path,
    ) {
        let Some(spawner) = self.spawners.get(&layer_index) else {
            return;
        };
        emit_particles(
            self.particles.entry(layer_index).or_default(),
            random,
            &spawner.emitter,
            // See the automatic path above: the native base vector remains
            // empty even while a derived layer bucket grows.
            0,
            Some((resources, data_root)),
        );
    }

    /// `sub_100096E4C` first clears the base Particles vector at `+0x40`, then
    /// clears the two derived trees at `+0x88` and `+0xB8`.  The base
    /// definition cache rooted at `+0x60` deliberately survives a refresh.
    pub(crate) fn clear(&mut self) {
        self.spawners.clear();
        self.particles.clear();
    }
}
