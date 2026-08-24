//! Inline `b2Body::ApplyForce` accumulation used by native sensor paths.

use crate::*;

impl SceneObject {
    pub(crate) fn apply_native_force_at(&mut self, force: (f32, f32), point: (f32, f32)) {
        if !self.dynamic_body {
            return;
        }
        let center = self.native_world_center();
        let torque = force
            .1
            .mul_add(point.0 - center.0, force.0 * (center.1 - point.1));
        self.force_x = f64::from((self.force_x as f32) + force.0);
        self.force_y = f64::from((self.force_y as f32) + force.1);
        self.torque = f64::from((self.torque as f32) + torque);
        self.wake();
        self.motion_started = true;
    }
}
