//! Scene-relative animation entity transform, sprite and bounds queries.

use super::{
    super::model::*, affine::animation_node_world_affine, hierarchy::animation_local_transform,
    skin::animation_slot_attachment,
};

fn animation_scene_affine(runtime: &AnimationRuntime, tag: &str) -> AnimationAffine {
    runtime
        .matrices
        .get(tag)
        .copied()
        .or_else(|| {
            runtime
                .transforms
                .get(tag)
                .copied()
                .map(AnimationAffine::from_transform)
        })
        .unwrap_or_default()
}

pub(crate) fn animation_entity_world_affine(
    runtime: &AnimationRuntime,
    tag: &str,
    entity: &str,
) -> Option<AnimationAffine> {
    let playback = runtime.playback.get(tag)?;
    let definition = runtime.definitions.get(tag)?;
    let scene = animation_scene_affine(runtime, tag);
    let descendant_reflection = runtime
        .descendant_reflections
        .get(tag)
        .copied()
        .unwrap_or(false);
    let entity_world =
        animation_node_world_affine(definition, playback, entity, scene, descendant_reflection)?;
    Some(scene.inverse().compose(entity_world))
}

pub(crate) fn animation_entity_local_transform(
    runtime: &AnimationRuntime,
    tag: &str,
    entity: &str,
) -> Option<AnimationTransform> {
    let playback = runtime.playback.get(tag)?;
    let definition = runtime.definitions.get(tag)?;
    animation_local_transform(definition, playback, entity)
}

pub(crate) fn animation_entity_has_sprite(
    runtime: &AnimationRuntime,
    tag: &str,
    entity: &str,
) -> Option<bool> {
    let playback = runtime.playback.get(tag)?;
    let definition = runtime.definitions.get(tag)?;
    if !definition.slots.iter().any(|slot| slot == entity) {
        return None;
    }
    if let Some(target) = playback
        .latched_targets
        .get(entity)
        .filter(|target| target.sprite_applied)
    {
        return Some(target.bound_sprite.is_some());
    }
    Some(
        animation_slot_attachment(
            definition,
            playback,
            runtime.skins.get(tag).map(String::as_str),
            entity,
        )
        .is_some(),
    )
}

pub(crate) fn animation_entity_world_bounds(
    runtime: &AnimationRuntime,
    tag: &str,
    entity: &str,
) -> [f64; 4] {
    let Some(playback) = runtime.playback.get(tag) else {
        return [0.0; 4];
    };
    let Some(definition) = runtime.definitions.get(tag) else {
        return [0.0; 4];
    };
    if !definition.slots.iter().any(|slot| slot == entity) {
        return [0.0; 4];
    }
    let (skin_transform, sprite_width, sprite_height) = if let Some(target) = playback
        .latched_targets
        .get(entity)
        .filter(|target| target.sprite_applied)
    {
        let Some(binding) = target.bound_sprite.as_ref() else {
            return [0.0; 4];
        };
        (
            binding.skin_transform.clone(),
            f64::from(binding.metrics.width),
            f64::from(binding.metrics.height),
        )
    } else {
        let Some((sprite, skin_transform)) = animation_slot_attachment(
            definition,
            playback,
            runtime.skins.get(tag).map(String::as_str),
            entity,
        ) else {
            return [0.0; 4];
        };
        let dimensions = runtime
            .sprite_metrics
            .get(tag)
            .and_then(|metrics| metrics.get(&sprite))
            .map(|metrics| (f64::from(metrics.width), f64::from(metrics.height)))
            .or_else(|| {
                runtime
                    .sprite_geometry
                    .get(tag)
                    .and_then(|geometry| geometry.get(&sprite))
                    .copied()
                    .map(|geometry| (geometry.width(), geometry.height()))
            });
        let Some((sprite_width, sprite_height)) = dimensions else {
            return [0.0; 4];
        };
        (skin_transform, sprite_width, sprite_height)
    };
    let scene = animation_scene_affine(runtime, tag);
    let descendant_reflection = runtime
        .descendant_reflections
        .get(tag)
        .copied()
        .unwrap_or(false);
    let Some(mut transform) =
        animation_node_world_affine(definition, playback, entity, scene, descendant_reflection)
    else {
        return [0.0; 4];
    };
    // sub_1000152BC finds the entity's SpriteComponent and derives the
    // bounds from its final scene-relative transform. SpriteComponentCustom
    // applies the selected skin attachment transform to that same entity, so
    // its offset and scale are part of the queried bounds just as they are in
    // draw(). ComicCutscene depends on this when it builds the four-border
    // clip rectangle: omitting the attachment collapses all border slots onto
    // their parent nodes and clips the entire comic page away.
    if let Some(skin_transform) = skin_transform {
        transform = transform.then_skin_attachment(&skin_transform);
    }
    // The native public query returns the sprite bounds in the wrapper
    // scene's coordinates even though Transform::GetWorldMatrix performs the
    // mirrored-parent correction while walking the actual world hierarchy.
    transform = scene.inverse().compose(transform);
    // sub_1000152BC intentionally uses the two basis-vector magnitudes and
    // the sprite's signed width/height, not the rotated four-corner AABB. Its
    // ARM64 path converts the dimensions to float32, multiplies by 0.5 first,
    // then multiplies by the already-rounded FSQRT result.
    let center_x = transform.x as f32;
    let center_y = transform.y as f32;
    let half_width = (sprite_width as f32 * 0.5_f32) * transform.scale_x() as f32;
    let half_height = (sprite_height as f32 * 0.5_f32) * transform.scale_y() as f32;
    [
        f64::from(center_x - half_width),
        f64::from(center_y - half_height),
        f64::from(center_x + half_width),
        f64::from(center_y + half_height),
    ]
}
