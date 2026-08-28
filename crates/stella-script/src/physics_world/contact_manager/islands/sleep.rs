//! `b2Island::Solve` sleep-time accumulation and island sleep transition.

use crate::*;

const NATIVE_ANGULAR_SLEEP_TOLERANCE_SQUARED: f32 = f32::from_bits(0x3A9F_B511);
const NATIVE_LINEAR_SLEEP_TOLERANCE_SQUARED: f32 = f32::from_bits(0x3B23_D70B);
const NATIVE_TIME_TO_SLEEP: f32 = f32::from_bits(0x3F00_0000);

fn native_linear_sleep_speed_squared(velocity_x: f32, velocity_y: f32) -> f32 {
    // 0x10086D500..0x10086D508 squares both SIMD lanes independently, then
    // FADDP adds the two rounded products. A mul_add changes the boundary.
    let velocity_x_squared = velocity_x * velocity_x;
    let velocity_y_squared = velocity_y * velocity_y;
    velocity_x_squared + velocity_y_squared
}

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
                <= NATIVE_ANGULAR_SLEEP_TOLERANCE_SQUARED
                && native_linear_sleep_speed_squared(velocity_x, velocity_y)
                    <= NATIVE_LINEAR_SLEEP_TOLERANCE_SQUARED;
            if below_tolerance {
                let sleep_time = (object.sleep_time as f32) + step;
                object.sleep_time = f64::from(sleep_time);
                minimum_sleep_time = minimum_sleep_time.min(sleep_time);
            } else {
                object.sleep_time = 0.0;
                minimum_sleep_time = 0.0;
            }
        }
        if positions_solved && minimum_sleep_time >= NATIVE_TIME_TO_SLEEP {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_sleep_thresholds_keep_native_float32_words() {
        assert_eq!(
            NATIVE_ANGULAR_SLEEP_TOLERANCE_SQUARED.to_bits(),
            0x3A9F_B511
        );
        assert_eq!(NATIVE_LINEAR_SLEEP_TOLERANCE_SQUARED.to_bits(), 0x3B23_D70B);
        assert_eq!(NATIVE_TIME_TO_SLEEP.to_bits(), 0x3F00_0000);
    }

    #[test]
    fn linear_sleep_speed_squares_each_lane_before_adding() {
        let velocity_x = f32::from_bits(0x3CF5_C20D);
        let velocity_y = f32::from_bits(0x3D23_D73C);
        let separated = native_linear_sleep_speed_squared(velocity_x, velocity_y);
        let fused = velocity_x.mul_add(velocity_x, velocity_y * velocity_y);
        assert_eq!(separated.to_bits(), 0x3B23_D70C);
        assert_eq!(fused.to_bits(), 0x3B23_D70B);
    }
}
