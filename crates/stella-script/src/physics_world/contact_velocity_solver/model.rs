//! Compact b2ContactSolver body state captured from the native body array.

use std::collections::{BTreeMap, HashMap};

use crate::{ContactKey, SceneObject};

use super::native_contact_velocity_write;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactBodyState {
    pub(crate) inverse_mass: f32,
    pub(crate) inverse_inertia: f32,
    pub(crate) center: (f32, f32),
    pub(crate) velocity: (f32, f32),
    pub(crate) angular_velocity: f32,
    pub(crate) friction: f32,
    pub(crate) restitution: f32,
    pub(crate) sensor: bool,
    pub(crate) moves_during_step: bool,
    pub(crate) active: bool,
    pub(crate) motion_started: bool,
    pub(crate) sleeping: bool,
}

/// The two body states and their native-array addressing for one constraint.
/// Keeping these values together mirrors the body-index pair retained by a
/// native b2ContactVelocityConstraint and avoids re-threading eight scalar
/// arguments through every impulse application.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactVelocityBodies<'a> {
    pub(crate) names: (&'a str, &'a str),
    pub(crate) indices: Option<(usize, usize)>,
    pub(crate) first: ContactBodyState,
    pub(crate) second: ContactBodyState,
}

/// One 36-byte point from Purple's 152-byte
/// `b2ContactVelocityConstraint`. The accumulated impulses remain in the
/// solver-local impulse record so StoreImpulses can publish them separately.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeContactVelocityPoint {
    pub(crate) first_radius: (f32, f32),
    pub(crate) second_radius: (f32, f32),
    pub(crate) normal_mass: f32,
    pub(crate) tangent_mass: f32,
    pub(crate) velocity_bias: f32,
}

/// Constraint scalars frozen by `b2ContactSolver::InitializeVelocityConstraints`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeContactVelocityConstraint {
    pub(crate) points: [NativeContactVelocityPoint; 2],
    pub(crate) normal: (f32, f32),
    /// Inverse of the two-point normal K matrix `(m11, m12, m22)`.
    pub(crate) normal_mass: (f32, f32, f32),
    /// Two-point normal K matrix `(k11, k12, k22)`.
    pub(crate) normal_k: (f32, f32, f32),
    pub(crate) first: ContactBodyState,
    pub(crate) second: ContactBodyState,
    pub(crate) friction: f32,
    pub(crate) point_count: usize,
}

impl ContactBodyState {
    pub(crate) fn capture(object: &SceneObject, fixture: usize) -> Self {
        Self {
            inverse_mass: object.inverse_mass_for_solver() as f32,
            inverse_inertia: object.inverse_inertia() as f32,
            center: object.native_world_center(),
            velocity: (object.velocity_x as f32, object.velocity_y as f32),
            angular_velocity: object.angular_velocity as f32,
            friction: object.fixture_friction(fixture) as f32,
            restitution: object.fixture_restitution(fixture) as f32,
            sensor: object.sensor,
            moves_during_step: object.moves_during_step(),
            active: object.active,
            motion_started: object.motion_started,
            sleeping: object.sleeping,
        }
    }

    pub(crate) fn participates_in_velocity_solve(self) -> bool {
        self.moves_during_step && self.active && self.motion_started && !self.sleeping
    }
}

/// Compact velocity array used while one native island contact pass runs.
/// Box2D constraints hold integer body indices into this array; the Rust
/// scene remains name-addressed for Lua, so the index is rebuilt at the
/// island boundary and committed before the next joint/track pass.
#[derive(Debug, Default)]
pub(crate) struct NativeContactVelocityCache {
    indices: HashMap<String, usize>,
    names: Vec<String>,
    velocities: Vec<(f32, f32, f32)>,
}

impl NativeContactVelocityCache {
    pub(crate) fn capture(
        scene: &BTreeMap<String, SceneObject>,
        contact_keys: &[ContactKey],
    ) -> Self {
        let mut cache = Self::default();
        for name in contact_keys
            .iter()
            .flat_map(|key| [key.0.as_str(), key.1.as_str()])
        {
            if cache.indices.contains_key(name) {
                continue;
            }
            let Some(object) = scene.get(name) else {
                continue;
            };
            let index = cache.velocities.len();
            cache.indices.insert(name.to_owned(), index);
            cache.names.push(name.to_owned());
            cache.velocities.push((
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            ));
        }
        cache
    }

    /// Refresh the compact native velocity array after the joint and track
    /// solvers have written their pass results back to the Lua-addressable
    /// scene. Purple keeps the body-index table for the whole island solve;
    /// only the three float32 velocity values are live between passes.
    pub(crate) fn refresh(&mut self, scene: &BTreeMap<String, SceneObject>) {
        for (name, velocity) in self.names.iter().zip(&mut self.velocities) {
            let Some(object) = scene.get(name) else {
                continue;
            };
            *velocity = (
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            );
        }
    }

    pub(crate) fn body_indices(&self, names: (&str, &str)) -> Option<(usize, usize)> {
        Some((*self.indices.get(names.0)?, *self.indices.get(names.1)?))
    }

    pub(crate) fn velocity_at(&self, index: usize) -> Option<(f32, f32, f32)> {
        self.velocities.get(index).copied()
    }

    pub(crate) fn velocity(&self, name: &str) -> Option<(f32, f32, f32)> {
        self.indices
            .get(name)
            .and_then(|index| self.velocities.get(*index))
            .copied()
    }

    pub(crate) fn apply_native_contact_impulse(
        &mut self,
        names: (&str, &str),
        first_coefficients: (f32, f32, f32),
        second_coefficients: (f32, f32, f32),
        impulse: (f32, f32),
    ) -> bool {
        let Some(first_index) = self.indices.get(names.0).copied() else {
            return false;
        };
        let Some(second_index) = self.indices.get(names.1).copied() else {
            return false;
        };
        self.apply_native_contact_impulse_at(
            (first_index, second_index),
            first_coefficients,
            second_coefficients,
            impulse,
        )
    }

    pub(crate) fn apply_native_contact_impulse_at(
        &mut self,
        indices: (usize, usize),
        first_coefficients: (f32, f32, f32),
        second_coefficients: (f32, f32, f32),
        impulse: (f32, f32),
    ) -> bool {
        let (first_index, second_index) = indices;
        if first_index == second_index {
            let Some(velocity) = self.velocities.get_mut(first_index) else {
                return false;
            };
            native_contact_velocity_write(
                velocity,
                first_coefficients.0,
                first_coefficients.1,
                impulse,
                first_coefficients.2,
            );
            native_contact_velocity_write(
                velocity,
                second_coefficients.0,
                second_coefficients.1,
                impulse,
                second_coefficients.2,
            );
            return true;
        }
        let Ok([first_velocity, second_velocity]) = self
            .velocities
            .get_disjoint_mut([first_index, second_index])
        else {
            return false;
        };
        native_contact_velocity_write(
            first_velocity,
            first_coefficients.0,
            first_coefficients.1,
            impulse,
            first_coefficients.2,
        );
        native_contact_velocity_write(
            second_velocity,
            second_coefficients.0,
            second_coefficients.1,
            impulse,
            second_coefficients.2,
        );
        true
    }

    pub(crate) fn commit(&self, scene: &mut BTreeMap<String, SceneObject>) {
        for (name, (velocity_x, velocity_y, angular_velocity)) in
            self.names.iter().zip(self.velocities.iter().copied())
        {
            if let Some(object) = scene.get_mut(name) {
                object.velocity_x = f64::from(velocity_x);
                object.velocity_y = f64::from(velocity_y);
                object.angular_velocity = f64::from(angular_velocity);
            }
        }
    }
}
