//! `b2Island::Solve` sleep-time accumulation and island sleep transition.

use crate::*;

impl RenderBridge {
    /// Apply the island-wide sleep branch at `0x10086D494..0x10086D584`.
    /// The minimum sleep time across a connected island controls every body;
    /// a single moving endpoint keeps all joint/contact neighbors awake.
    #[cfg(test)]
    pub(crate) fn update_box2d_island_sleep(&mut self, step: f64, positions_solved: bool) {
        let islands = self.solver_islands.clone();
        for island in &islands {
            self.update_single_box2d_island_sleep(island, step, positions_solved);
        }
    }

    pub(crate) fn update_single_box2d_island_sleep(
        &mut self,
        island: &SolverIsland,
        step: f64,
        positions_solved: bool,
    ) {
        const LINEAR_SLEEP_TOLERANCE_SQUARED: f32 = 0.0025;
        const ANGULAR_SLEEP_TOLERANCE_SQUARED: f32 = 0.00121847;
        const TIME_TO_SLEEP: f32 = 0.5;

        let step = step as f32;
        let mut minimum_sleep_time = f32::MAX;
        for name in &island.bodies {
            let Some(object) = self.scene.get_mut(name) else {
                continue;
            };
            // The native branch skips b2_staticBody before checking the
            // allow-sleep bit or accumulating the island minimum.
            if !object.moves_during_step() {
                continue;
            }
            let angular_velocity = object.angular_velocity as f32;
            let velocity_x = object.velocity_x as f32;
            let velocity_y = object.velocity_y as f32;
            let below_tolerance = angular_velocity * angular_velocity
                <= ANGULAR_SLEEP_TOLERANCE_SQUARED
                && velocity_x.mul_add(velocity_x, velocity_y * velocity_y)
                    <= LINEAR_SLEEP_TOLERANCE_SQUARED;
            if below_tolerance {
                let sleep_time = (object.sleep_time as f32) + step;
                object.sleep_time = f64::from(sleep_time);
                minimum_sleep_time = minimum_sleep_time.min(sleep_time);
            } else {
                object.sleep_time = 0.0;
                minimum_sleep_time = 0.0;
            }
        }
        if positions_solved && minimum_sleep_time >= TIME_TO_SLEEP {
            for name in &island.bodies {
                if let Some(object) = self.scene.get_mut(name) {
                    object.sleeping = true;
                    object.sleep_time = 0.0;
                    object.velocity_x = 0.0;
                    object.velocity_y = 0.0;
                    object.angular_velocity = 0.0;
                }
            }
        }
    }
}
