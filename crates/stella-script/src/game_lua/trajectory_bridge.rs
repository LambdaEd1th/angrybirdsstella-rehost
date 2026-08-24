//! AimStream runtime methods attached to the aggregate bridge.

use crate::*;

impl RenderBridge {
    /// `AimStream::reset` (`sub_1000086CC`) is called by native
    /// `loadLevelImpl` after publishing the new world. The reset owns only
    /// AimStream's two vectors and active flag; the spawn timer is retained.
    pub(crate) fn reset_native_aim_stream_for_level_load(&mut self) {
        self.aim_stream_particles.clear();
        self.aim_stream_control_points.clear();
        self.aim_stream_active = false;
    }

    /// `sub_1000675C0`, called from `loadLevelImpl` at `0x100066228`, replaces
    /// GameLua's vector with two default 0x38-byte trajectory records and
    /// resets the selected record to zero. This is a separate native lifetime
    /// boundary from AimStream::reset.
    pub(crate) fn reset_native_flight_trails_for_level_load(&mut self) {
        self.trajectory_streams = [
            NativeTrajectoryBuffer::default(),
            NativeTrajectoryBuffer::default(),
        ];
        self.trajectory_stream_index = 0;
    }

    pub(crate) fn populate_native_aim_stream(&mut self, spawn_time: f32, speed: f32) {
        self.aim_stream_particles.clear();
        let segment_count = self.aim_stream_control_points.len() as i32 - 3;
        if segment_count > 0 {
            let segment_count_f32 = segment_count as f32;
            let denominator = spawn_time * speed;
            if denominator.is_finite() && denominator > 0.0 {
                // sub_10000839C uses FCVTZS after the float32 division.
                let particle_count = native_fcvtzs_f32(segment_count_f32 / denominator);
                for index in 0..particle_count.max(0) {
                    let index_f32 = index as f32;
                    let path_parameter = (index_f32 * speed) * spawn_time;
                    let angle = (index_f32 * 0.0_f32) * spawn_time;
                    let scale = (1.2_f32 - path_parameter / segment_count_f32)
                        * self.game_world_scale as f32;
                    self.aim_stream_particles.push(NativeAimParticle {
                        path_parameter,
                        angle,
                        scale,
                    });
                }
            }
        }
        self.aim_stream_spawn_timer = spawn_time;
    }

    pub(crate) fn set_native_aim_stream_active(
        &mut self,
        active: bool,
        spawn_time: f32,
        speed: f32,
    ) {
        // AimStream::setActive repopulates only on a false -> true edge.
        if self.aim_stream_active != active && active {
            self.populate_native_aim_stream(spawn_time, speed);
        }
        self.aim_stream_active = active;
    }

    pub(crate) fn update_native_aim_stream(&mut self, delta: f32, spawn_time: f32, speed: f32) {
        let segment_count = self.aim_stream_control_points.len() as i32 - 3;
        if segment_count > 0 {
            let segment_count_f32 = segment_count as f32;
            let game_world_scale = self.game_world_scale as f32;
            self.aim_stream_particles.retain_mut(|particle| {
                particle.path_parameter = speed.mul_add(delta, particle.path_parameter);
                // AimStream+0x4C is initialized to zero and has no shipped
                // writer, but native still executes the fused update.
                particle.angle = 0.0_f32.mul_add(delta, particle.angle);
                particle.scale =
                    (1.2_f32 - particle.path_parameter / segment_count_f32) * game_world_scale;
                particle.path_parameter < segment_count_f32
            });
        }

        self.aim_stream_spawn_timer -= delta;
        if self.aim_stream_active
            && self.aim_stream_spawn_timer < 0.0
            && spawn_time.is_finite()
            && spawn_time > 0.0
        {
            let scale = 1.2_f32 * self.game_world_scale as f32;
            while self.aim_stream_spawn_timer < 0.0 {
                self.aim_stream_spawn_timer += spawn_time;
                self.aim_stream_particles.push(NativeAimParticle {
                    path_parameter: 0.0,
                    angle: 0.0,
                    scale,
                });
            }
        }
    }
}
