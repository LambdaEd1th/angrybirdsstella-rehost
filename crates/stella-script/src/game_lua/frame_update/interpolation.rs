//! Native two-slot RenderObjectData interpolation in `sub_10005E898`.

use crate::*;

const INTERPOLATION_RECIPROCAL_STEP: f32 = f32::from_bits(0x41EF_FFFF);
const NATIVE_PI: f32 = f32::from_bits(0x4049_0FDB);

impl RenderBridge {
    /// On each retained tail Box2D step, Purple flips GameLua+0x23c and
    /// snapshots every awake body's transform into that RenderObjectData
    /// pose slot. Catch-up steps are skipped only while over two steps remain,
    /// leaving the final two consecutive solutions available to rendering.
    pub(crate) fn capture_native_interpolation_frame(&mut self) {
        self.physics_interpolation_slot = usize::from(self.physics_interpolation_slot == 0);
        let slot = self.physics_interpolation_slot;
        for object in self.scene.values_mut() {
            if object.has_physics_body() && !object.sleeping {
                object.capture_native_interpolation_pose(slot);
            }
        }
    }

    /// `0x10005F07C..0x10005F168` blends the preceding and current solved
    /// slots into RenderObjectData +0xA4/+0xA8/+0xAC on every unlocked frame.
    /// The accumulator, reciprocal, products and FMADDs are all float32.
    pub(crate) fn interpolate_native_scene_poses(&mut self) {
        let alpha = self.physics_accumulator * INTERPOLATION_RECIPROCAL_STEP;
        let previous_weight = 1.0_f32 - alpha;
        let current_slot = self.physics_interpolation_slot;
        let previous_slot = usize::from(current_slot == 0);

        for object in self.scene.values_mut() {
            if !object.has_physics_body() || object.sleeping {
                continue;
            }
            let current = object.interpolation_poses[current_slot];
            let previous = object.interpolation_poses[previous_slot];
            let x = alpha.mul_add(current.x, previous_weight * previous.x);
            let y = alpha.mul_add(current.y, previous_weight * previous.y);

            let angle_delta = current.angle - previous.angle;
            let previous_angle = if angle_delta.abs() < NATIVE_PI {
                previous.angle
            } else if current.angle > previous.angle {
                previous.angle + (NATIVE_PI + NATIVE_PI)
            } else {
                previous.angle - (NATIVE_PI + NATIVE_PI)
            };
            let angle = alpha.mul_add(current.angle, previous_weight * previous_angle);

            object.render_x = f64::from(x);
            object.render_y = f64::from(y);
            object.render_angle = f64::from(angle);
        }
    }
}
