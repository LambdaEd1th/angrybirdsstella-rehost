//! RenderObjectData's explicit-pose reset and fixed-step snapshot helpers.

use crate::*;

impl SceneObject {
    /// `sub_10003FA60` writes an explicit position to the live render pose and
    /// both 12-byte interpolation slots after updating the b2Body transform.
    pub(crate) fn reset_native_interpolation_position(&mut self, x: f32, y: f32) {
        self.render_x = f64::from(x);
        self.render_y = f64::from(y);
        for pose in &mut self.interpolation_poses {
            pose.x = x;
            pose.y = y;
        }
    }

    /// `sub_10003FB78` performs the equivalent three writes for rotation.
    pub(crate) fn reset_native_interpolation_angle(&mut self, angle: f32) {
        self.render_angle = f64::from(angle);
        for pose in &mut self.interpolation_poses {
            pose.angle = angle;
        }
    }

    pub(crate) fn capture_native_interpolation_pose(&mut self, slot: usize) {
        // sub_10005EFF4 reads b2Transform::q and normalizes the captured
        // angle with atan2f(sin, cos), keeping every slot inside [-PI, PI].
        let (sine, cosine) = (self.angle as f32).sin_cos();
        self.interpolation_poses[slot] = NativeInterpolationPose {
            x: self.x as f32,
            y: self.y as f32,
            angle: sine.atan2(cosine),
        };
        self.display_interpolation_velocities[slot] =
            DisplayInterpolationVelocity::new(self.velocity_x, self.velocity_y);
    }

    /// Keep the visual velocity pair coherent when a Lua/native member changes
    /// b2Body velocity outside World::Step. The physics value itself remains
    /// authoritative and is still exported only at the recovered frame tail.
    pub(crate) fn reset_display_interpolation_velocity(&mut self, x: f32, y: f32) {
        self.display_interpolation_velocities = [DisplayInterpolationVelocity { x, y }; 2];
    }
}
