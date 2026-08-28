//! Track constraints and float32 island velocity integration.

use crate::*;

fn native_track_endpoint_impulse(edge: (f32, f32), fraction: f32, base: (f32, f32)) -> (f32, f32) {
    // sub_10086DC30..60 first folds an out-of-range endpoint correction
    // directly into the accumulated normal impulse with FMADD. FCSEL's
    // unordered path reaches the below-start expression as well.
    if fraction >= 0.0_f32 {
        if fraction <= 1.0_f32 {
            base
        } else {
            let excess = fraction + -1.0_f32;
            (
                (-edge.0).mul_add(excess, base.0),
                (-edge.1).mul_add(excess, base.1),
            )
        }
    } else {
        (
            (-edge.0).mul_add(fraction, base.0),
            (-edge.1).mul_add(fraction, base.1),
        )
    }
}

impl RenderBridge {
    pub(crate) fn begin_island_track_step(&mut self, body_names: &[String]) {
        let track_names = self
            .tracks
            .keys()
            .filter(|name| body_names.contains(name))
            .cloned()
            .collect::<Vec<_>>();
        for name in track_names {
            let Some(object) = self.scene.get(&name) else {
                self.tracks.remove(&name);
                continue;
            };
            let position = (object.x, object.y);
            let Some(track) = self.tracks.get_mut(&name) else {
                continue;
            };
            let old_segment = track.current_segment;
            let Some(edge) = track.native_closest_edge(position) else {
                track.current_segment = -1;
                track.impulse_x = 0.0;
                track.impulse_y = 0.0;
                continue;
            };
            track.current_segment = edge.index;
            track.edge_start = edge.start;
            track.edge_end = edge.end;
            track.angle = edge.angle;
            if edge.index != old_segment {
                track.impulse_x = 0.0;
                track.impulse_y = 0.0;
                continue;
            }

            // sub_10086DA58 warm-starts only when the closest child is
            // unchanged. The fixed step has a native dtRatio of one.
            let impulse_x = track.impulse_x as f32;
            let impulse_y = track.impulse_y as f32;
            if let Some(object) = self.scene.get_mut(&name) {
                object.velocity_x = f64::from(object.velocity_x as f32 + impulse_x);
                object.velocity_y = f64::from(object.velocity_y as f32 + impulse_y);
                object.angular_velocity = f64::from((object.angular_velocity as f32) * 0.9_f32);
            }
        }
    }

    pub(crate) fn solve_island_track_velocity_constraints(&mut self, body_names: &[String]) {
        let track_names = self
            .tracks
            .keys()
            .filter(|name| body_names.contains(name))
            .cloned()
            .collect::<Vec<_>>();
        for name in track_names {
            let Some(track) = self.tracks.get(&name) else {
                continue;
            };
            if track.current_segment < 0 {
                continue;
            }
            let edge_start = (track.edge_start.0 as f32, track.edge_start.1 as f32);
            let edge_end = (track.edge_end.0 as f32, track.edge_end.1 as f32);
            let track_angle = track.angle as f32;
            let rotate_block = track.rotate_block;
            let Some(object) = self.scene.get_mut(&name) else {
                continue;
            };
            let position_x = object.x as f32;
            let position_y = object.y as f32;
            let body_angle = object.angle as f32;
            let mut velocity_x = object.velocity_x as f32;
            let mut velocity_y = object.velocity_y as f32;
            let mut angular_velocity = object.angular_velocity as f32;

            let cosine = track_angle.cos();
            let sine = track_angle.sin();
            // FNMSUB at sub_10086DBCC computes vy*cos - vx*sin. Multiplying
            // that by (sin,-cos) removes ten percent of normal velocity.
            let normal_correction = velocity_y.mul_add(cosine, -(velocity_x * sine)) * 0.1_f32;
            let base_x = sine * normal_correction;
            let base_y = -cosine * normal_correction;
            if let Some(track) = self.tracks.get_mut(&name) {
                track.impulse_x = f64::from(track.impulse_x as f32 + base_x);
                track.impulse_y = f64::from(track.impulse_y as f32 + base_y);
            }

            let edge_x = edge_end.0 - edge_start.0;
            let edge_y = edge_end.1 - edge_start.1;
            let length_squared = edge_x.mul_add(edge_x, edge_y * edge_y);
            let fraction = (position_x - edge_start.0)
                .mul_add(edge_x, (position_y - edge_start.1) * edge_y)
                / length_squared;
            let (linear_impulse_x, linear_impulse_y) =
                native_track_endpoint_impulse((edge_x, edge_y), fraction, (base_x, base_y));
            let projected_x = edge_x.mul_add(fraction, edge_start.0);
            let projected_y = edge_y.mul_add(fraction, edge_start.1);
            let correction_x = (projected_x - position_x).mul_add(0.1_f32, linear_impulse_x);
            let correction_y = (projected_y - position_y).mul_add(0.1_f32, linear_impulse_y);
            velocity_x += correction_x;
            velocity_y += correction_y;

            let target_angle = if rotate_block { track_angle } else { -0.0_f32 };
            angular_velocity = (target_angle - body_angle)
                .mul_add(0.2_f32, angular_velocity * -0.2_f32)
                + angular_velocity;
            object.velocity_x = f64::from(velocity_x);
            object.velocity_y = f64::from(velocity_y);
            object.angular_velocity = f64::from(angular_velocity);
        }
    }

    /// Integrate one island's dynamic-body velocities exactly as the leading
    /// loop in `sub_10086CE84`. Purple keeps every operand in float32, uses
    /// fused multiply-adds for gravity/force and clamps both damping factors
    /// to [0, 1].
    pub(crate) fn integrate_island_velocities(
        &mut self,
        body_names: &[String],
        gravity: (f64, f64),
        step: f64,
    ) {
        let step = step as f32;
        let gravity_x = gravity.0 as f32;
        let gravity_y = gravity.1 as f32;
        for name in body_names {
            let Some(object) = self.scene.get_mut(name) else {
                continue;
            };
            if !object.dynamic_body || !object.active || object.sleeping || !object.motion_started {
                continue;
            }

            let inverse_mass = object.inverse_mass as f32;
            let gravity_scale = object.gravity_scale as f32;
            let force_acceleration_x = inverse_mass * object.force_x as f32;
            let force_acceleration_y = inverse_mass * object.force_y as f32;
            let acceleration_x = gravity_scale.mul_add(gravity_x, force_acceleration_x);
            let acceleration_y = gravity_scale.mul_add(gravity_y, force_acceleration_y);
            let mut velocity_x = step.mul_add(acceleration_x, object.velocity_x as f32);
            let mut velocity_y = step.mul_add(acceleration_y, object.velocity_y as f32);

            let angular_step = step * object.inverse_inertia() as f32;
            let mut angular_velocity =
                angular_step.mul_add(object.torque as f32, object.angular_velocity as f32);
            let linear_drag = (-step)
                .mul_add(object.linear_damping as f32, 1.0_f32)
                .clamp(0.0_f32, 1.0_f32);
            velocity_x *= linear_drag;
            velocity_y *= linear_drag;
            let angular_drag = (-step)
                .mul_add(object.angular_damping as f32, 1.0_f32)
                .clamp(0.0_f32, 1.0_f32);
            angular_velocity *= angular_drag;

            object.velocity_x = f64::from(velocity_x);
            object.velocity_y = f64::from(velocity_y);
            object.angular_velocity = f64::from(angular_velocity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::native_track_endpoint_impulse;

    #[test]
    fn track_endpoint_impulse_folds_the_base_into_native_fmadd() {
        let edge = f32::from_bits(0x3F43_B7B2);
        let fraction = f32::from_bits(0x4072_3369);
        let base = f32::from_bits(0x3EF8_65B4);
        let native = native_track_endpoint_impulse((edge, 0.0), fraction, (base, 0.0)).0;
        let separated = base + edge * (1.0_f32 - fraction);

        assert_eq!(native.to_bits(), 0xBFD2_60A2);
        assert_eq!(separated.to_bits(), 0xBFD2_60A3);
    }

    #[test]
    fn unordered_track_fraction_follows_the_native_below_start_path() {
        let impulse = native_track_endpoint_impulse((2.0, -3.0), f32::NAN, (1.0, 4.0));

        assert!(impulse.0.is_nan());
        assert!(impulse.1.is_nan());
    }
}
