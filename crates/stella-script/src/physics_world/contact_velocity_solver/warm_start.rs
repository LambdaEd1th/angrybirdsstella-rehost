//! `b2ContactSolver::WarmStart` cached impulse application.

use crate::*;

impl RenderBridge {
    pub(crate) fn warm_start_island_contact_constraints(&mut self, contact_keys: &[ContactKey]) {
        for pair in contact_keys {
            let Some(constraint) = self.contact_velocity_constraints.get(pair).copied() else {
                continue;
            };
            let impulse = self
                .solver_contact_impulses
                .get(pair)
                .copied()
                .unwrap_or_default();
            self.apply_cached_contact_impulse(pair, constraint, impulse);
        }
    }

    fn apply_cached_contact_impulse(
        &mut self,
        pair: &ContactKey,
        constraint: NativeContactVelocityConstraint,
        impulse: CachedContactImpulse,
    ) {
        let normal = constraint.normal;
        let tangent = (normal.1, -normal.0);
        let bodies = ContactVelocityBodies {
            names: (&pair.0, &pair.1),
            indices: None,
            first: constraint.first,
            second: constraint.second,
        };
        for (index, point) in constraint
            .points
            .into_iter()
            .take(constraint.point_count)
            .enumerate()
        {
            let (normal_impulse, tangent_impulse) = impulse.point(index);
            let normal_impulse = normal_impulse as f32;
            let tangent_impulse = tangent_impulse as f32;
            let total_impulse = (
                normal_impulse.mul_add(normal.0, tangent_impulse * tangent.0),
                normal_impulse.mul_add(normal.1, tangent_impulse * tangent.1),
            );
            self.apply_contact_velocity_impulse(bodies, point, total_impulse);
        }
    }
}
