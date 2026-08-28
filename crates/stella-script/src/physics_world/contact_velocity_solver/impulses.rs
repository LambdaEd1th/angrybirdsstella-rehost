//! Scalar and paired contact-impulse application helpers.

use crate::*;

use super::{
    native_contact_block_linear_impulse, native_contact_cross, native_contact_velocity_write,
};

impl RenderBridge {
    pub(crate) fn begin_contact_velocity_cache(&mut self, contact_keys: &[ContactKey]) {
        self.contact_velocity_cache = Some(NativeContactVelocityCache::capture(
            &self.scene,
            contact_keys,
        ));
    }

    pub(crate) fn refresh_contact_velocity_cache(&mut self) {
        if let Some(cache) = self.contact_velocity_cache.as_mut() {
            cache.refresh(&self.scene);
        }
    }

    pub(crate) fn commit_contact_velocity_cache(&mut self) {
        if let Some(cache) = self.contact_velocity_cache.as_ref() {
            cache.commit(&mut self.scene);
        }
    }

    pub(crate) fn end_contact_velocity_cache(&mut self) {
        self.contact_velocity_cache = None;
    }

    pub(crate) fn relative_contact_velocity(
        &self,
        bodies: ContactVelocityBodies<'_>,
        point: NativeContactVelocityPoint,
    ) -> (f32, f32) {
        let ContactVelocityBodies {
            names: (first_name, second_name),
            indices: body_indices,
            first,
            second,
        } = bodies;
        let first_radius = point.first_radius;
        let second_radius = point.second_radius;
        let cached = self.contact_velocity_cache.as_ref();
        let first_live = body_indices
            .and_then(|indices| cached.and_then(|cache| cache.velocity_at(indices.0)))
            .or_else(|| cached.and_then(|cache| cache.velocity(first_name)))
            .or_else(|| {
                self.scene.get(first_name).map(|object| {
                    (
                        object.velocity_x as f32,
                        object.velocity_y as f32,
                        object.angular_velocity as f32,
                    )
                })
            });
        let second_live = body_indices
            .and_then(|indices| cached.and_then(|cache| cache.velocity_at(indices.1)))
            .or_else(|| cached.and_then(|cache| cache.velocity(second_name)))
            .or_else(|| {
                self.scene.get(second_name).map(|object| {
                    (
                        object.velocity_x as f32,
                        object.velocity_y as f32,
                        object.angular_velocity as f32,
                    )
                })
            });
        let first_angular_velocity = first_live.map_or(first.angular_velocity, |live| live.2);
        let second_angular_velocity = second_live.map_or(second.angular_velocity, |live| live.2);
        let first_velocity = first_live.map_or(first.velocity, |live| (live.0, live.1));
        let second_velocity = second_live.map_or(second.velocity, |live| (live.0, live.1));
        let first_velocity = (
            (-first_angular_velocity).mul_add(first_radius.1, first_velocity.0),
            first_angular_velocity.mul_add(first_radius.0, first_velocity.1),
        );
        let second_velocity = (
            (-second_angular_velocity).mul_add(second_radius.1, second_velocity.0),
            second_angular_velocity.mul_add(second_radius.0, second_velocity.1),
        );
        (
            second_velocity.0 - first_velocity.0,
            second_velocity.1 - first_velocity.1,
        )
    }

    pub(crate) fn apply_contact_velocity_impulse(
        &mut self,
        bodies: ContactVelocityBodies<'_>,
        point: NativeContactVelocityPoint,
        impulse: (f32, f32),
    ) {
        let ContactVelocityBodies {
            names: (first_name, second_name),
            indices: body_indices,
            first,
            second,
        } = bodies;
        let first_inverse_mass = first.inverse_mass;
        let second_inverse_mass = second.inverse_mass;
        let first_inverse_inertia = first.inverse_inertia;
        let second_inverse_inertia = second.inverse_inertia;
        let first_radius = point.first_radius;
        let second_radius = point.second_radius;
        let first_cross = native_contact_cross(first_radius, impulse);
        let second_cross = native_contact_cross(second_radius, impulse);
        let first_coefficients = (-first_inverse_mass, -first_inverse_inertia, first_cross);
        let second_coefficients = (second_inverse_mass, second_inverse_inertia, second_cross);
        if self.contact_velocity_cache.as_mut().is_some_and(|cache| {
            if let Some(indices) = body_indices {
                cache.apply_native_contact_impulse_at(
                    indices,
                    first_coefficients,
                    second_coefficients,
                    impulse,
                )
            } else {
                cache.apply_native_contact_impulse(
                    (first_name, second_name),
                    first_coefficients,
                    second_coefficients,
                    impulse,
                )
            }
        }) {
            return;
        }
        if let Some(object) = self.scene.get_mut(first_name) {
            let mut velocity = (
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            );
            native_contact_velocity_write(
                &mut velocity,
                first_coefficients.0,
                first_coefficients.1,
                impulse,
                first_coefficients.2,
            );
            (
                object.velocity_x,
                object.velocity_y,
                object.angular_velocity,
            ) = (
                f64::from(velocity.0),
                f64::from(velocity.1),
                f64::from(velocity.2),
            );
        }
        if let Some(object) = self.scene.get_mut(second_name) {
            let mut velocity = (
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            );
            native_contact_velocity_write(
                &mut velocity,
                second_coefficients.0,
                second_coefficients.1,
                impulse,
                second_coefficients.2,
            );
            (
                object.velocity_x,
                object.velocity_y,
                object.angular_velocity,
            ) = (
                f64::from(velocity.0),
                f64::from(velocity.1),
                f64::from(velocity.2),
            );
        }
    }

    pub(crate) fn apply_contact_block_velocity_impulse(
        &mut self,
        bodies: ContactVelocityBodies<'_>,
        points: [NativeContactVelocityPoint; 2],
        normal: (f32, f32),
        deltas: [f32; 2],
    ) {
        let ContactVelocityBodies {
            names,
            indices: body_indices,
            first,
            second,
        } = bodies;
        let first_inverse_mass = first.inverse_mass;
        let second_inverse_mass = second.inverse_mass;
        let first_inverse_inertia = first.inverse_inertia;
        let second_inverse_inertia = second.inverse_inertia;
        let impulses = [
            (deltas[0] * normal.0, deltas[0] * normal.1),
            (deltas[1] * normal.0, deltas[1] * normal.1),
        ];
        let total_impulse = native_contact_block_linear_impulse(normal, deltas);
        let first_cross = native_contact_cross(points[0].first_radius, impulses[0])
            + native_contact_cross(points[1].first_radius, impulses[1]);
        let second_cross = native_contact_cross(points[0].second_radius, impulses[0])
            + native_contact_cross(points[1].second_radius, impulses[1]);
        let first_coefficients = (-first_inverse_mass, -first_inverse_inertia, first_cross);
        let second_coefficients = (second_inverse_mass, second_inverse_inertia, second_cross);
        if self.contact_velocity_cache.as_mut().is_some_and(|cache| {
            if let Some(indices) = body_indices {
                cache.apply_native_contact_impulse_at(
                    indices,
                    first_coefficients,
                    second_coefficients,
                    total_impulse,
                )
            } else {
                cache.apply_native_contact_impulse(
                    names,
                    first_coefficients,
                    second_coefficients,
                    total_impulse,
                )
            }
        }) {
            return;
        }
        if let Some(object) = self.scene.get_mut(names.0) {
            let mut velocity = (
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            );
            native_contact_velocity_write(
                &mut velocity,
                first_coefficients.0,
                first_coefficients.1,
                total_impulse,
                first_coefficients.2,
            );
            (
                object.velocity_x,
                object.velocity_y,
                object.angular_velocity,
            ) = (
                f64::from(velocity.0),
                f64::from(velocity.1),
                f64::from(velocity.2),
            );
        }
        if let Some(object) = self.scene.get_mut(names.1) {
            let mut velocity = (
                object.velocity_x as f32,
                object.velocity_y as f32,
                object.angular_velocity as f32,
            );
            native_contact_velocity_write(
                &mut velocity,
                second_coefficients.0,
                second_coefficients.1,
                total_impulse,
                second_coefficients.2,
            );
            (
                object.velocity_x,
                object.velocity_y,
                object.angular_velocity,
            ) = (
                f64::from(velocity.0),
                f64::from(velocity.1),
                f64::from(velocity.2),
            );
        }
    }
}
