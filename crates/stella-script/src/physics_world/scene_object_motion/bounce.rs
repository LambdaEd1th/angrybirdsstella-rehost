//! Game-side collision bounce amplitude member (`sub_100062520`).

use crate::*;

impl SceneObject {
    pub(crate) fn trigger_native_bounce(&mut self, collision_impulse: f64) {
        if self.bounce_amplitude_multiplier <= 0.0 {
            return;
        }
        let amplitude = ((collision_impulse as f32) * 0.02_f32).min(0.1_f32);
        if amplitude > self.bounce_current_amplitude as f32 {
            self.bounce_active = true;
            self.bounce_initial_amplitude = f64::from(amplitude);
        }
    }
}
