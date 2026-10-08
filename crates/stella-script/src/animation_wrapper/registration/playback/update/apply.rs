//! Ordered EntityTarget state sampling and component/matrix writes (10041E41C).

use std::path::Path;

use crate::*;

fn discrete_state_index<T>(track: &[(f64, T)], time: f64) -> Option<usize> {
    if track.is_empty() {
        return None;
    }
    let time = time as f32;
    let upper = track.partition_point(|(key_time, _)| (*key_time as f32) <= time);
    Some(upper.saturating_sub(1).min(track.len() - 1))
}

struct SpriteBindingContext<'a> {
    definition: &'a AnimationDefinition,
    skins: Option<&'a BTreeMap<String, AnimationSkin>>,
    selected_skin: Option<&'a str>,
    regions: Option<&'a BTreeMap<String, SpriteCatalogRegion>>,
    root_present: bool,
    resources: Option<&'a ResourceRuntime>,
    data_root: Option<&'a Path>,
}

fn resolve_native_sprite_binding(
    context: &SpriteBindingContext<'_>,
    slot: &str,
    alias: &str,
    kind: AnimationSpriteTrackKind,
) -> (Option<AnimationBoundSprite>, Option<AnimationAffine>) {
    let attachment = match kind {
        AnimationSpriteTrackKind::DirectSprite => {
            Some((alias.rsplit('/').next().unwrap_or(alias).to_owned(), None))
        }
        AnimationSpriteTrackKind::SkinAlias => {
            let skins = match context.skins {
                Some(skins) => skins,
                // Only manually assembled fixtures without a native wrapper
                // owner retain the definition's concrete-region setup path.
                None if !context.root_present => &context.definition.skins,
                None => return (None, None),
            };
            animation_skin_alias_attachment(skins, context.selected_skin, slot, alias)
        }
    };
    let Some((sprite, skin_transform)) = attachment else {
        return (None, None);
    };
    // 100011BC4 writes the attachment matrix whenever the exact skin record
    // exists, including when its live provider returns a null Sprite*.
    let local_matrix = skin_transform
        .as_ref()
        .map(AnimationAffine::from_skin_attachment);
    let region = match kind {
        AnimationSpriteTrackKind::DirectSprite => context
            .regions
            .and_then(|regions| regions.get(&sprite))
            .cloned(),
        AnimationSpriteTrackKind::SkinAlias => {
            if let Some((resources, data_root)) = context.resources.zip(context.data_root) {
                resources
                    .active_atlas_catalog_region(&sprite, data_root)
                    .map(|region| (*region).clone())
            } else if context.resources.is_none() {
                context
                    .regions
                    .and_then(|regions| regions.get(&sprite))
                    .cloned()
            } else {
                None
            }
        }
    };
    let binding = region.map(|region| {
        let atlas = &region.sprite;
        AnimationBoundSprite {
            sprite,
            skin_transform,
            metrics: NativeSpriteMetrics {
                width: i32::from(atlas.width),
                height: i32::from(atlas.height),
                pivot_x: i32::from(atlas.pivot_x),
                pivot_y: i32::from(atlas.pivot_y),
            },
            region,
        }
    });
    (binding, local_matrix)
}

fn latch_native_targets(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    mode: u8,
) -> Vec<AnimationTimelineEvent> {
    let Some(definition) = runtime.definitions.get(tag) else {
        return Vec::new();
    };
    let context = SpriteBindingContext {
        definition,
        skins: runtime.skin_sets.get(tag),
        selected_skin: runtime.skins.get(tag).map(String::as_str),
        regions: runtime.sprite_regions.get(tag),
        root_present: runtime.root_present,
        resources,
        data_root,
    };
    let Some(playback) = runtime.playback.get_mut(tag) else {
        return Vec::new();
    };
    if playback.target_groups.is_none() {
        let mut groups = BTreeMap::new();
        for control in &playback.controls {
            if let Some(action) = definition.actions.get(&control.action) {
                attach_animation_target_states(&mut groups, &control.action, action);
            }
        }
        playback.target_groups = Some(groups);
    }
    let groups = playback
        .target_groups
        .as_mut()
        .expect("entity target groups initialized");
    let mut events = Vec::new();
    for (entity, groups) in groups {
        for group in groups {
            let Some(state) = group.states.last_mut() else {
                continue;
            };
            let Some(control) = playback
                .controls
                .iter()
                .find(|control| control.action == state.action)
            else {
                continue;
            };
            let Some(action) = definition.actions.get(&state.action) else {
                continue;
            };
            let previous = state.elapsed;
            let delta = control.elapsed as f32 - previous;
            // StateBase::update returns false without invoking its timeline
            // when the absolute float delta is zero (10041D354). Each State
            // retains its own time; Control vector swaps cannot select it.
            if mode == 3 && delta.abs() <= 0.0 {
                continue;
            }
            let time = previous + delta;
            state.elapsed = time;
            let time = f64::from(time);
            if group.usage == AnimationUsage::SpineEvent {
                let event = if mode == 3 {
                    animation_event_after_state_change(action, f64::from(previous), time)
                } else {
                    animation_event_at(action, time)
                };
                if let Some(event) = event {
                    events.push(event);
                }
                continue;
            }
            let Some(track) = action.targets.get(entity) else {
                continue;
            };
            let target = playback.latched_targets.entry(entity.clone()).or_default();
            match group.usage {
                AnimationUsage::Translation => {
                    let value = sample_float2(&track.translation, time, [0.0, 0.0]);
                    target.translation = value;
                    target
                        .local_matrix
                        .get_or_insert_default()
                        .set_translation(value[0], value[1]);
                }
                AnimationUsage::Rotation => {
                    let value = sample_float(&track.rotation, time, 0.0);
                    target.rotation = value;
                    target
                        .local_matrix
                        .get_or_insert_default()
                        .set_rotation(value);
                }
                AnimationUsage::Scale => {
                    let value = sample_float2(&track.scale, time, [1.0, 1.0]);
                    target.scale = value;
                    target
                        .local_matrix
                        .get_or_insert_default()
                        .set_scale(value[0], value[1]);
                }
                AnimationUsage::Alpha => target.alpha = sample_float(&track.alpha, time, 1.0),
                AnimationUsage::Sprite => {
                    if (mode != 3
                        || discrete_state_index(&track.sprite, f64::from(previous))
                            != discrete_state_index(&track.sprite, time))
                        && let Some(value) = sample_discrete(&track.sprite, time)
                    {
                        let (binding, matrix) = resolve_native_sprite_binding(
                            &context,
                            entity,
                            &value,
                            track.sprite_kind,
                        );
                        target.sprite = value;
                        target.sprite_applied = true;
                        target.bound_sprite = binding;
                        if let Some(matrix) = matrix {
                            target.local_matrix = Some(matrix);
                        }
                    }
                }
                AnimationUsage::ZOrder => {
                    if (mode != 3
                        || discrete_state_index(&track.z_order, f64::from(previous))
                            != discrete_state_index(&track.z_order, time))
                        && let Some(value) = sample_discrete(&track.z_order, time)
                    {
                        target.z_order = value;
                    }
                }
                AnimationUsage::SpineEvent => {
                    unreachable!("event group handled before entity track lookup")
                }
            }
        }
    }
    events
}

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
    let events = latch_native_targets(runtime, resources, data_root, tag, mode);
    // The start path's mode 0 is EntityTarget::apply, not Animation::apply.
    // Only the latter copies the entire control vector's previous times
    // after visiting targets (100410A18/1004111A4).
    if mode != 0
        && let Some(playback) = runtime.playback.get_mut(tag)
    {
        for control in &mut playback.controls {
            control.previous_elapsed = control.elapsed;
        }
    }
    for event in events {
        queue_animation_event(runtime, tag, event);
    }
}
