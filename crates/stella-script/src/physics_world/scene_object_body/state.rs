//! `b2Body` awake and mass-state members, including ResetMassData.

use crate::*;

impl SceneObject {
    pub(crate) fn wake(&mut self) {
        self.sleeping = false;
        self.sleep_time = 0.0;
    }

    pub(crate) fn reset_native_mass_data(&mut self, old_world_center: (f64, f64)) {
        // b2Body::ResetMassData discards any previous SetMassData inertia.
        self.moment_of_inertia = None;
        // The native body only traverses its fixture list for dynamic bodies
        // (type 2). Static and kinematic bodies clear m_mass/m_I and return
        // before ComputeMass; their fixture list densities remain available
        // for the next dynamic ResetMassData call. Keep the previous
        // aggregate in that branch instead of eagerly recomputing it.
        if self.dynamic_body {
            self.fixture_mass_data = self.compute_native_fixture_mass_data_f32();
        }
        let mass = self.fixture_mass_data.0;
        self.body_mass = if self.dynamic_body && mass > 0.0_f32 {
            mass
        } else if self.dynamic_body {
            1.0_f32
        } else {
            0.0_f32
        };
        self.inverse_mass = if self.body_mass > 0.0_f32 {
            f64::from(self.body_mass.recip())
        } else {
            0.0
        };
        if self.dynamic_body {
            let center = self.fixture_mass_data.1;
            self.native_local_center_x = center.0;
            self.native_local_center_y = center.1;
        } else {
            // 0x10086B210 zeroes localCenter before the type test. Types zero
            // and one return at 0x10086B238 without traversing fixture mass.
            self.native_local_center_x = 0.0;
            self.native_local_center_y = 0.0;
        }
        // ResetMassData writes the new local centre, transforms it into
        // b2Sweep::c/c0, and only then applies the COM velocity shift.
        self.sync_native_sweep_from_transform();
        self.preserve_velocity_after_mass_reset(old_world_center);
    }

    /// Apply the `b2MassData::I` value accepted by b2Body::SetMassData. The
    /// public value is about the transform origin; Box2D stores COM inertia.
    pub(crate) fn set_native_mass_data_from_origin_inertia(&mut self, inertia: f32) {
        if !self.dynamic_body {
            return;
        }
        if inertia > 0.0_f32 && !self.fixed_rotation {
            let center = self.local_center();
            let center_x = center.0 as f32;
            let center_y = center.1 as f32;
            let center_squared = center_x.mul_add(center_x, center_y * center_y);
            let com_inertia = (-self.body_mass).mul_add(center_squared, inertia);
            self.moment_of_inertia = Some(f64::from(com_inertia));
        } else {
            self.moment_of_inertia = Some(0.0);
        }
    }

    pub(crate) fn inverse_inertia(&self) -> f64 {
        if !self.dynamic_body || self.fixed_rotation || self.inverse_mass <= f64::EPSILON {
            return 0.0;
        }
        if let Some(moment) = self.moment_of_inertia {
            let moment = moment as f32;
            return if moment != 0.0_f32 {
                f64::from(moment.recip())
            } else {
                0.0
            };
        }
        let inertia = self.native_fixture_mass_data_f32().2;
        if inertia > 0.0_f32 {
            f64::from(inertia.recip())
        } else {
            0.0
        }
    }

    pub(crate) fn inverse_mass_for_solver(&self) -> f64 {
        if self.dynamic_body {
            self.inverse_mass
        } else {
            0.0
        }
    }

    pub(crate) fn native_body_mass(&self) -> f32 {
        if self.dynamic_body {
            self.body_mass
        } else {
            0.0
        }
    }

    pub(crate) fn native_shape_dimensions(&self) -> (f32, f32) {
        let (width, height) = match self.collision_shape {
            CollisionShape::Circle { .. } => {
                let diameter = self.native_shape_radius.abs() * 2.0;
                (diameter, diameter)
            }
            _ => (
                self.native_shape_width.abs(),
                self.native_shape_height.abs(),
            ),
        };
        (width as f32, height as f32)
    }
}
