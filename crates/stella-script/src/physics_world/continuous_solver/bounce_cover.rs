//! Preserve the shipped drum's non-damaging front bounce at thin membranes.
//!
//! Ordinary dynamic/dynamic eligibility remains native. This narrow exception
//! uses the recovered TOI/manifold math to stop a bird at the cover rather than
//! let a discrete step penetrate both the cover and its backing drum base.

use crate::*;

fn pair<'a>(
    first: &'a SceneObject,
    second: &'a SceneObject,
) -> Option<(&'a SceneObject, &'a SceneObject, bool)> {
    let (cover, bird, cover_is_first) = if first.continuous_bounce_cover && second.controllable {
        (first, second, true)
    } else if second.continuous_bounce_cover && first.controllable {
        (second, first, false)
    } else {
        return None;
    };
    (!cover.controllable
        && matches!(cover.collision_shape, CollisionShape::Box { .. })
        && matches!(bird.collision_shape, CollisionShape::Circle { .. }))
    .then_some((cover, bird, cover_is_first))
}

pub(super) fn is_bounce_cover_pair(first: &SceneObject, second: &SceneObject) -> bool {
    pair(first, second).is_some()
}

pub(super) fn is_front_bounce_contact(
    first: &SceneObject,
    second: &SceneObject,
    first_start: NativeSweepStart,
    second_start: NativeSweepStart,
    manifold: ContactManifold,
) -> bool {
    let Some((cover, bird, cover_is_first)) = pair(first, second) else {
        return false;
    };
    let (cover_start, bird_start, bird_radius, cover_radius) = if cover_is_first {
        (
            first_start,
            second_start,
            manifold.position.second_radius,
            manifold.position.first_radius,
        )
    } else {
        (
            second_start,
            first_start,
            manifold.position.first_radius,
            manifold.position.second_radius,
        )
    };
    let cover_transform =
        cover.native_collision_transform_at_sweep(cover_start.center, cover_start.angle);
    let bird_transform =
        bird.native_collision_transform_at_sweep(bird_start.center, bird_start.angle);
    let start = cover_transform.inverse_point(bird_transform.position);
    let end = cover
        .native_collision_transform()
        .inverse_point(bird.native_collision_transform().position);
    let half_height =
        (cover.native_shape_height as f32 * 0.5) * (cover.physics_scale_y as f32).abs();
    // Start outside the front surface, move towards it, and hit its outward
    // face. Side/underside/base hits keep their ordinary damage and abilities.
    // Polygon-circle local normals belong to the polygon in either factory
    // order, so this test has no name-order or world-angle special case.
    start.1 <= -half_height - bird_radius - cover_radius
        && end.1 > start.1
        && manifold.position.local_normal.1 < 0.0
}
