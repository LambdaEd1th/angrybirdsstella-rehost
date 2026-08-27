//! Entity-target selection and property application (sub_10041E41C).

use std::path::Path;

use crate::*;

fn native_apply_event(
    runtime: &AnimationRuntime,
    tag: &str,
    mode: u8,
) -> Option<AnimationTimelineEvent> {
    let playback = runtime.playback.get(tag)?;
    let definition = runtime.definitions.get(tag)?;
    let (control, action) = playback.controls.iter().rev().find_map(|control| {
        let action = definition.actions.get(&control.action)?;
        (!action.event_track.is_empty()).then_some((control, action))
    })?;
    if mode == 3 {
        animation_event_after_state_change(action, control.previous_elapsed, control.elapsed)
    } else {
        animation_event_at(action, control.elapsed)
    }
}

#[derive(Default)]
struct AnimationLatchedUpdate {
    translation: Option<[f64; 2]>,
    scale: Option<[f64; 2]>,
    rotation: Option<f64>,
    alpha: Option<f64>,
    sprite_claimed: bool,
    sprite: Option<(String, AnimationSpriteTrackKind)>,
    z_order_claimed: bool,
    z_order: Option<i64>,
}

fn discrete_state_index<T>(track: &[(f64, T)], time: f64) -> Option<usize> {
    if track.is_empty() {
        return None;
    }
    let time = time as f32;
    let upper = track.partition_point(|(key_time, _)| (*key_time as f32) <= time);
    Some(upper.saturating_sub(1).min(track.len() - 1))
}

fn resolve_native_sprite_binding(
    runtime: &AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    slot: &str,
    alias: &str,
    kind: AnimationSpriteTrackKind,
) -> Option<AnimationBoundSprite> {
    let definition = runtime.definitions.get(tag)?;
    let (sprite, skin_transform) = match kind {
        AnimationSpriteTrackKind::DirectSprite => {
            (alias.rsplit('/').next().unwrap_or(alias).to_owned(), None)
        }
        AnimationSpriteTrackKind::SkinAlias => animation_skin_alias_attachment(
            definition,
            runtime.skins.get(tag).map(String::as_str),
            slot,
            alias,
        )?,
    };
    let region = match kind {
        AnimationSpriteTrackKind::DirectSprite => runtime
            .sprite_regions
            .get(tag)
            .and_then(|regions| regions.get(&sprite))
            .cloned(),
        AnimationSpriteTrackKind::SkinAlias => {
            if let Some((resources, data_root)) = resources.zip(data_root) {
                resources
                    .active_atlas_catalog_region(&sprite, data_root)
                    .map(|region| (*region).clone())
            } else if resources.is_none() {
                // Unit fixtures without a ResourceRuntime retain the previous
                // concrete-region setup path. Installed Lua methods always
                // pass the live manager and cannot use a stale region.
                runtime
                    .sprite_regions
                    .get(tag)
                    .and_then(|regions| regions.get(&sprite))
                    .cloned()
            } else {
                None
            }
        }
    }?;
    let atlas = &region.sprite;
    Some(AnimationBoundSprite {
        sprite,
        skin_transform,
        metrics: NativeSpriteMetrics {
            width: i32::from(atlas.width),
            height: i32::from(atlas.height),
            pivot_x: i32::from(atlas.pivot_x),
            pivot_y: i32::from(atlas.pivot_y),
        },
        region,
    })
}

fn latch_native_targets(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    mode: u8,
) {
    let Some(playback) = runtime.playback.get(tag) else {
        return;
    };
    let Some(definition) = runtime.definitions.get(tag) else {
        return;
    };
    let mut updates = BTreeMap::<String, AnimationLatchedUpdate>::new();
    // EntityTarget stores one ordered state vector per usage. Walking active
    // controls backwards finds the same last state selected by
    // sub_10041E41C, independently for every property.
    for control in playback.controls.iter().rev() {
        let Some(action) = definition.actions.get(&control.action) else {
            continue;
        };
        for (entity, target) in &action.targets {
            let update = updates.entry(entity.clone()).or_default();
            if update.translation.is_none() && !target.translation.is_empty() {
                update.translation = Some(sample_float2(
                    &target.translation,
                    control.elapsed,
                    [0.0, 0.0],
                ));
            }
            if update.scale.is_none() && !target.scale.is_empty() {
                update.scale = Some(sample_float2(&target.scale, control.elapsed, [1.0, 1.0]));
            }
            if update.rotation.is_none() && !target.rotation.is_empty() {
                update.rotation = Some(sample_float(&target.rotation, control.elapsed, 0.0));
            }
            if update.alpha.is_none() && !target.alpha.is_empty() {
                update.alpha = Some(sample_float(&target.alpha, control.elapsed, 1.0));
            }
            if !update.sprite_claimed && !target.sprite.is_empty() {
                // The last active State owns the entire usage even when its
                // discrete keyframe index did not change. Falling through to
                // an older control here would run the wrong ApplyHandler.
                update.sprite_claimed = true;
                let should_apply = mode != 3
                    || discrete_state_index(&target.sprite, control.previous_elapsed)
                        != discrete_state_index(&target.sprite, control.elapsed);
                if should_apply {
                    update.sprite = sample_discrete(&target.sprite, control.elapsed)
                        .map(|sprite| (sprite, target.sprite_kind));
                }
            }
            if !update.z_order_claimed && !target.z_order.is_empty() {
                update.z_order_claimed = true;
                let should_apply = mode != 3
                    || discrete_state_index(&target.z_order, control.previous_elapsed)
                        != discrete_state_index(&target.z_order, control.elapsed);
                if should_apply {
                    update.z_order = sample_discrete(&target.z_order, control.elapsed);
                }
            }
        }
    }
    for (entity, update) in updates {
        let resolved_sprite = update.sprite.as_ref().and_then(|(sprite, kind)| {
            resolve_native_sprite_binding(
                runtime, resources, data_root, tag, &entity, sprite, *kind,
            )
        });
        let Some(playback) = runtime.playback.get_mut(tag) else {
            return;
        };
        let target = playback.latched_targets.entry(entity).or_default();
        if let Some(value) = update.translation {
            target.translation = value;
        }
        if let Some(value) = update.scale {
            target.scale = value;
        }
        if let Some(value) = update.rotation {
            target.rotation = value;
        }
        if let Some(value) = update.alpha {
            target.alpha = value;
        }
        if let Some((value, _)) = update.sprite {
            target.sprite = value;
            target.sprite_applied = true;
            target.bound_sprite = resolved_sprite;
        }
        if let Some(value) = update.z_order {
            target.z_order = value;
        }
    }
}

/// `Animation::apply(mode)` visits each EntityTarget once. Every usage group
/// selects the last attached state, then the animation copies every control's
/// current time into its previous-time field (`sub_1004111A4`).
pub(in crate::animation_wrapper::registration::playback) fn apply_native_targets(
    runtime: &mut AnimationRuntime,
    tag: &str,
    mode: u8,
) {
    apply_native_targets_with_context(runtime, None, None, tag, mode);
}

pub(in crate::animation_wrapper::registration::playback) fn apply_native_targets_with_resources(
    runtime: &mut AnimationRuntime,
    resources: &ResourceRuntime,
    data_root: &Path,
    tag: &str,
    mode: u8,
) {
    apply_native_targets_with_context(runtime, Some(resources), Some(data_root), tag, mode);
}

pub(super) fn apply_native_targets_with_context(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    mode: u8,
) {
    let event = native_apply_event(runtime, tag, mode);
    latch_native_targets(runtime, resources, data_root, tag, mode);
    if let Some(playback) = runtime.playback.get_mut(tag) {
        for control in &mut playback.controls {
            control.previous_elapsed = control.elapsed;
        }
    }
    if let Some(event) = event {
        queue_animation_event(runtime, tag, event);
    }
}
