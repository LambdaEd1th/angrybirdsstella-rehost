//! Per-material rolling-volume aggregation inside `sub_10005E898`.

use crate::*;

impl RenderBridge {
    pub(crate) fn native_rolling_audio_levels(&self) -> [f32; 3] {
        let mut levels = [0.0_f32; 3];
        for object in self.scene.values() {
            // RenderObjectData+0x140 excludes controllable birds and +0x147
            // admits only the native circle constructor/fixture path.
            if object.controllable
                || !matches!(object.collision_shape, CollisionShape::Circle { .. })
            {
                continue;
            }
            let Ok(material) = usize::try_from(object.native_material - 1) else {
                continue;
            };
            let Some(level) = levels.get_mut(material) else {
                continue;
            };

            // 0x10005F698..0x10005F6C8 performs four separate float32
            // operations: abs(angularVelocity) * radius * 0.0025f * mass,
            // then FMIN with one. Do not fuse or reorder this sequence.
            let angular_speed = (object.angular_velocity as f32).abs();
            let radius_scaled = angular_speed * object.native_shape_radius as f32;
            let gain_scaled = radius_scaled * f32::from_bits(0x3B23_D70A);
            let candidate = (gain_scaled * object.body_mass).min(1.0_f32);
            if *level < candidate {
                *level = candidate;
            }
        }
        levels
    }
}
