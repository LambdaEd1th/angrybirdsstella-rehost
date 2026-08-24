//! Typed track sampling and parent-chain compatibility transforms.

use super::super::model::*;

pub(crate) fn animation_node_transform(
    definition: &AnimationDefinition,
    playback: &AnimationPlayback,
    entity: &str,
    root: AnimationTransform,
) -> Option<AnimationTransform> {
    if !animation_definition_contains_entity(definition, entity) {
        return None;
    }
    let mut path = vec![entity];
    let mut current = entity;
    while let Some(parent) = definition.parents.get(current) {
        path.push(parent);
        current = parent;
    }
    path.reverse();
    let mut world = root;
    for name in path {
        let local = animation_local_transform(definition, playback, name)?;
        let local_x = local.x * world.scale_x;
        let local_y = local.y * world.scale_y;
        let cosine = world.angle.cos();
        let sine = world.angle.sin();
        world.x += local_x * cosine - local_y * sine;
        world.y += local_x * sine + local_y * cosine;
        world.scale_x *= local.scale_x;
        world.scale_y *= local.scale_y;
        world.angle += local.angle;
    }
    Some(world)
}

pub(crate) fn animation_definition_contains_entity(
    definition: &AnimationDefinition,
    entity: &str,
) -> bool {
    definition.entities.contains(entity)
        || definition.parents.contains_key(entity)
        || definition.parents.values().any(|parent| parent == entity)
        || definition.slots.iter().any(|slot| slot == entity)
        || definition
            .actions
            .values()
            .any(|action| action.targets.contains_key(entity))
}

pub(crate) fn animation_local_transform(
    definition: &AnimationDefinition,
    playback: &AnimationPlayback,
    entity: &str,
) -> Option<AnimationTransform> {
    if !animation_definition_contains_entity(definition, entity) {
        return None;
    }
    let translation = animation_target_sample(definition, playback, entity, |target| {
        !target.translation.is_empty()
    })
    .map(|(target, time)| sample_float2(&target.translation, time, [0.0, 0.0]))
    .or_else(|| {
        playback
            .latched_targets
            .get(entity)
            .map(|target| target.translation)
    })
    .unwrap_or([0.0, 0.0]);
    let scale = animation_target_sample(definition, playback, entity, |target| {
        !target.scale.is_empty()
    })
    .map(|(target, time)| sample_float2(&target.scale, time, [1.0, 1.0]))
    .or_else(|| {
        playback
            .latched_targets
            .get(entity)
            .map(|target| target.scale)
    })
    .unwrap_or([1.0, 1.0]);
    let angle = animation_target_sample(definition, playback, entity, |target| {
        !target.rotation.is_empty()
    })
    .map(|(target, time)| sample_float(&target.rotation, time, 0.0))
    .or_else(|| {
        playback
            .latched_targets
            .get(entity)
            .map(|target| target.rotation)
    })
    .unwrap_or(0.0);
    Some(AnimationTransform {
        x: translation[0],
        y: translation[1],
        scale_x: scale[0],
        scale_y: scale[1],
        angle,
    })
}

/// EntityTarget keeps one ordered State vector for each timeline usage and
/// applies only its last state (`sub_10041E41C`). Controls are appended on
/// first start, reused in place on later starts, and swap-removed on stop.
pub(crate) fn animation_target_sample<'a>(
    definition: &'a AnimationDefinition,
    playback: &AnimationPlayback,
    entity: &str,
    has_track: impl Fn(&AnimationTarget) -> bool,
) -> Option<(&'a AnimationTarget, f64)> {
    for control in playback.controls.iter().rev() {
        let Some(target) = definition
            .actions
            .get(&control.action)
            .and_then(|action| action.targets.get(entity))
        else {
            continue;
        };
        if has_track(target) {
            return Some((target, control.elapsed));
        }
    }
    None
}
