//! Scalar and paired contact-impulse application helpers.

use crate::*;

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
        first_name: &str,
        second_name: &str,
        body_indices: Option<(usize, usize)>,
        first: ContactBodyState,
        second: ContactBodyState,
        point: ContactPoint,
    ) -> (f32, f32) {
        let first_center = first.center;
        let second_center = second.center;
        let first_radius = (
            point.point_x as f32 - first_center.0,
            point.point_y as f32 - first_center.1,
        );
        let second_radius = (
            point.point_x as f32 - second_center.0,
            point.point_y as f32 - second_center.1,
        );
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
        point: ContactPoint,
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
        let first_center = first.center;
        let second_center = second.center;
        let first_radius = (
            point.point_x as f32 - first_center.0,
            point.point_y as f32 - first_center.1,
        );
        let second_radius = (
            point.point_x as f32 - second_center.0,
            point.point_y as f32 - second_center.1,
        );
        let first_cross = first_radius
            .0
            .mul_add(impulse.1, -(first_radius.1 * impulse.0));
        let second_cross = second_radius
            .0
            .mul_add(impulse.1, -(second_radius.1 * impulse.0));
        let first_delta = (
            -(first_inverse_mass * impulse.0),
            -(first_inverse_mass * impulse.1),
            -(first_inverse_inertia * first_cross),
        );
        let second_delta = (
            second_inverse_mass * impulse.0,
            second_inverse_mass * impulse.1,
            second_inverse_inertia * second_cross,
        );
        if self.contact_velocity_cache.as_mut().is_some_and(|cache| {
            if let Some(indices) = body_indices {
                cache.apply_impulse_at(indices, first_delta, second_delta)
            } else {
                cache.apply_impulse((first_name, second_name), first_delta, second_delta)
            }
        }) {
            return;
        }
        if let Some(object) = self.scene.get_mut(first_name) {
            object.velocity_x =
                f64::from(object.velocity_x as f32 - first_inverse_mass * impulse.0);
            object.velocity_y =
                f64::from(object.velocity_y as f32 - first_inverse_mass * impulse.1);
            object.angular_velocity =
                f64::from(object.angular_velocity as f32 - first_inverse_inertia * first_cross);
        }
        if let Some(object) = self.scene.get_mut(second_name) {
            object.velocity_x =
                f64::from(object.velocity_x as f32 + second_inverse_mass * impulse.0);
            object.velocity_y =
                f64::from(object.velocity_y as f32 + second_inverse_mass * impulse.1);
            object.angular_velocity =
                f64::from(object.angular_velocity as f32 + second_inverse_inertia * second_cross);
        }
    }

    pub(crate) fn apply_contact_block_velocity_impulse(
        &mut self,
        bodies: ContactVelocityBodies<'_>,
        points: [ContactPoint; 2],
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
        let first_center = first.center;
        let second_center = second.center;
        let impulses = [
            (deltas[0] * normal.0, deltas[0] * normal.1),
            (deltas[1] * normal.0, deltas[1] * normal.1),
        ];
        let total_impulse = (impulses[0].0 + impulses[1].0, impulses[0].1 + impulses[1].1);
        let first_cross = (points[0].point_x as f32 - first_center.0).mul_add(
            impulses[0].1,
            -((points[0].point_y as f32 - first_center.1) * impulses[0].0),
        ) + (points[1].point_x as f32 - first_center.0).mul_add(
            impulses[1].1,
            -((points[1].point_y as f32 - first_center.1) * impulses[1].0),
        );
        let second_cross = (points[0].point_x as f32 - second_center.0).mul_add(
            impulses[0].1,
            -((points[0].point_y as f32 - second_center.1) * impulses[0].0),
        ) + (points[1].point_x as f32 - second_center.0).mul_add(
            impulses[1].1,
            -((points[1].point_y as f32 - second_center.1) * impulses[1].0),
        );
        let first_delta = (
            -(first_inverse_mass * total_impulse.0),
            -(first_inverse_mass * total_impulse.1),
            -(first_inverse_inertia * first_cross),
        );
        let second_delta = (
            second_inverse_mass * total_impulse.0,
            second_inverse_mass * total_impulse.1,
            second_inverse_inertia * second_cross,
        );
        if self.contact_velocity_cache.as_mut().is_some_and(|cache| {
            if let Some(indices) = body_indices {
                cache.apply_impulse_at(indices, first_delta, second_delta)
            } else {
                cache.apply_impulse(names, first_delta, second_delta)
            }
        }) {
            return;
        }
        if let Some(object) = self.scene.get_mut(names.0) {
            object.velocity_x =
                f64::from(object.velocity_x as f32 - first_inverse_mass * total_impulse.0);
            object.velocity_y =
                f64::from(object.velocity_y as f32 - first_inverse_mass * total_impulse.1);
            object.angular_velocity =
                f64::from(object.angular_velocity as f32 - first_inverse_inertia * first_cross);
        }
        if let Some(object) = self.scene.get_mut(names.1) {
            object.velocity_x =
                f64::from(object.velocity_x as f32 + second_inverse_mass * total_impulse.0);
            object.velocity_y =
                f64::from(object.velocity_y as f32 + second_inverse_mass * total_impulse.1);
            object.angular_velocity =
                f64::from(object.angular_velocity as f32 + second_inverse_inertia * second_cross);
        }
    }
}
