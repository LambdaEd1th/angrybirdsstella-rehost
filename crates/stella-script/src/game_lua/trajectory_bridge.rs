//! AimStream runtime methods attached to the aggregate bridge.

use crate::*;

impl RenderBridge {
    /// `loadLevelImpl` stores GameLua+0x4F8/+0x4FC/+0x500 immediately before
    /// configuring AimStream. The predictor consumes these retained values;
    /// it does not consult `worldAttributes` between predictions.
    pub(crate) fn load_native_simulation_settings(
        &mut self,
        iterations: i32,
        time_step_multiplier: f32,
        point_sampler: i32,
    ) {
        self.simulation_iterations = iterations;
        self.simulation_time_step_multiplier = time_step_multiplier;
        self.simulation_store_points_sampler = point_sampler;
    }

    /// `AimStream::reset` (`sub_1000086CC`) is called by native
    /// `loadLevelImpl` after publishing the new world. The reset owns only
    /// AimStream's two vectors and active flag; the spawn timer is retained.
    pub(crate) fn reset_native_aim_stream_for_level_load(&mut self) {
        self.aim_stream_particles.clear();
        self.aim_stream_control_points.clear();
        self.aim_stream_active = false;
    }

    /// `loadLevelImpl` writes AimStream+0x40/+0x48 before calling reset and
    /// setActive(false). The timer at +0x44 deliberately survives this
    /// boundary until the next population.
    pub(crate) fn load_native_aim_stream_settings(&mut self, spawn_time: f32, speed: f32) {
        self.aim_stream_spawn_time = spawn_time;
        self.aim_stream_speed = speed;
        self.reset_native_aim_stream_for_level_load();
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

    pub(crate) fn populate_native_aim_stream(&mut self) {
        self.aim_stream_particles.clear();
        let spawn_time = self.aim_stream_spawn_time;
        let speed = self.aim_stream_speed;
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

    pub(crate) fn set_native_aim_stream_active(&mut self, active: bool) {
        // AimStream::setActive repopulates only on a false -> true edge.
        if self.aim_stream_active != active && active {
            self.populate_native_aim_stream();
        }
        self.aim_stream_active = active;
    }

    pub(crate) fn update_native_aim_stream(&mut self, delta: f32) {
        let spawn_time = self.aim_stream_spawn_time;
        let speed = self.aim_stream_speed;
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
        if self.aim_stream_active && self.aim_stream_spawn_timer < 0.0 {
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
