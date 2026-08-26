//! Packed `ParticleData` integration recovered at `sub_100091834`.

use crate::*;

impl RenderBridge {
    #[cfg(test)]
    pub(crate) fn update_particles(&mut self, scaled_delta: f64, raw_delta: f64) {
        self.update_particles_inner(scaled_delta, raw_delta, None);
    }

    pub(crate) fn update_particles_with_resources(
        &mut self,
        scaled_delta: f64,
        raw_delta: f64,
        resources: &ResourceRuntime,
        data_root: &Path,
    ) {
        self.update_particles_inner(scaled_delta, raw_delta, Some((resources, data_root)));
    }

    fn update_particles_inner(
        &mut self,
        scaled_delta: f64,
        raw_delta: f64,
        bindings: Option<(&ResourceRuntime, &Path)>,
    ) {
        let scaled_delta = scaled_delta as f32;
        let raw_delta = raw_delta as f32;
        let physics_enabled = self.physics_enabled;
        let [x_min, x_max, y_min, y_max] = self.particle_wrap_limits.map(|bound| bound as f32);
        self.particle_system.particles.retain_mut(|particle| {
            // GameLua passes `(physicsLockTotal == 0)` as W2 at 0x1000605C0.
            // The disabled branch walks only modes 3 and 4; the independent
            // GameLua+0x199 flag controls drawing, not particle integration.
            if !physics_enabled && !matches!(particle.mode, 3 | 4) {
                return true;
            }
            let delta = if particle.ignore_time_multiplier {
                raw_delta
            } else {
                scaled_delta
            };
            particle.elapsed += delta;
            // sub_100091834 erases an expired finite particle before applying
            // gravity or advancing any of its remaining fields. Exactly -1f
            // is the native infinite-lifetime sentinel.
            if particle.elapsed > particle.lifetime && particle.lifetime != -1.0_f32 {
                return false;
            }
            particle.velocity_x = particle.gravity_x.mul_add(delta, particle.velocity_x);
            particle.velocity_y = particle.gravity_y.mul_add(delta, particle.velocity_y);
            // GameLua fixes W1 to one at 0x1000605C8, selecting the exact
            // scale-one branch. ThemeParticleSystem reuses this function with
            // W1 clear and is the only caller that applies height / 768.
            let movement_x = delta * particle.velocity_x;
            let movement_y = delta * particle.velocity_y;
            particle.x = 1.0_f32.mul_add(movement_x, particle.x);
            particle.y = 1.0_f32.mul_add(movement_y, particle.y);
            particle.angle = particle.angular_velocity.mul_add(delta, particle.angle);
            let progress = particle.elapsed / particle.lifetime;
            particle.current_scale =
                (particle.scale_end - particle.scale_begin).mul_add(progress, particle.scale_begin);

            // A definition whose animation string is exactly "lifeTime"
            // starts on sprite zero, then selects ceil(progress*count)-1.
            // The stored native frame number is one-based and clamped.
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
            // Infinite particles wrap against GameLua's signed +0x618..+0x624
            // bounds after integration. Native uses strict comparisons, so a
            // particle exactly on an edge is retained there.
            if particle.lifetime == -1.0_f32 {
                if particle.x < x_min {
                    particle.x = x_max;
                } else if particle.x > x_max {
                    particle.x = x_min;
                }
                if particle.y > y_max {
                    particle.y = y_min;
                } else if particle.y < y_min {
                    particle.y = y_max;
                }
            }
            true
        });
    }
}
