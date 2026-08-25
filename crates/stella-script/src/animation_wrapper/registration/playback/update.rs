//! Frame advancement and queued playback-event dispatch.

use std::path::Path;
use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::*;

fn native_repeat_mode(mode: &str) -> bool {
    mode.is_empty() || mode == "repeat"
}

fn advance_native_once(control: &AnimationControl, delta_time: f64) -> (f64, bool) {
    // Animation::Update at sub_100411230 performs all control arithmetic in
    // float32. State 3 is deliberately asymmetric: ordinary playback only
    // completes at the upper duration boundary. A control already exactly at
    // its duration (including a zero-duration static action) remains active
    // and is not completed again.
    let current = control.elapsed as f32;
    let duration = control.duration as f32;
    let delta = (delta_time as f32) * (control.speed as f32);
    let remaining = duration - current;
    if remaining > 0.0 {
        if delta >= remaining {
            (f64::from(duration), true)
        } else {
            (f64::from(current + delta), false)
        }
    } else if remaining < 0.0 {
        if delta >= -remaining {
            (f64::from(duration), true)
        } else {
            (f64::from(current - delta), false)
        }
    } else {
        (f64::from(current), false)
    }
}

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
                resources.active_atlas_catalog_region(&sprite, data_root)
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
pub(super) fn apply_native_targets(runtime: &mut AnimationRuntime, tag: &str, mode: u8) {
    apply_native_targets_with_context(runtime, None, None, tag, mode);
}

pub(super) fn apply_native_targets_with_resources(
    runtime: &mut AnimationRuntime,
    resources: &ResourceRuntime,
    data_root: &Path,
    tag: &str,
    mode: u8,
) {
    apply_native_targets_with_context(runtime, Some(resources), Some(data_root), tag, mode);
}

fn apply_native_targets_with_context(
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

fn retire_finished_control(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    index: usize,
) {
    let Some(playback) = runtime.playback.get_mut(tag) else {
        return;
    };
    if index >= playback.controls.len() {
        return;
    }
    let mut control = playback.controls.swap_remove(index);
    control.elapsed = 0.0;
    control.previous_elapsed = 0.0;
    control.playing = false;
    control.paused = false;
    control.finished_pending_removal = false;
    if control.action == playback.current_action {
        playback.detached_current = Some(control);
    }
    // sub_100410EA0 seeks the removed control, which forces the remaining
    // target states to apply at their current times.
    apply_native_targets_with_context(runtime, resources, data_root, tag, 2);
}

/// Advance every active control in native vector order. Completion callbacks
/// read the wrapper component's shared mode, even when another action is the
/// control that reached its endpoint.
pub(super) fn advance_native_scene(runtime: &mut AnimationRuntime, tag: &str, delta_time: f64) {
    advance_native_scene_with_context(runtime, None, None, tag, delta_time);
}

pub(super) fn advance_native_scene_with_resources(
    runtime: &mut AnimationRuntime,
    resources: &ResourceRuntime,
    data_root: &Path,
    tag: &str,
    delta_time: f64,
) {
    advance_native_scene_with_context(runtime, Some(resources), Some(data_root), tag, delta_time);
}

fn advance_native_scene_with_context(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    delta_time: f64,
) {
    let mut index = 0;
    loop {
        let Some(control_count) = runtime
            .playback
            .get(tag)
            .map(|playback| playback.controls.len())
        else {
            return;
        };
        if index >= control_count {
            break;
        }
        let remove = runtime.playback[tag].controls[index].finished_pending_removal;
        if remove {
            retire_finished_control(runtime, resources, data_root, tag, index);
            continue;
        }
        let completion = {
            let control = &mut runtime
                .playback
                .get_mut(tag)
                .expect("animation playback disappeared")
                .controls[index];
            if !control.playing || control.paused || control.speed == 0.0 {
                None
            } else {
                let (end_time, completed) = advance_native_once(control, delta_time);
                control.elapsed = end_time;
                completed.then_some(control.callback_installed)
            }
        };
        if let Some(callback_installed) = completion {
            if callback_installed {
                let mode = runtime.playback[tag].mode.clone();
                let event_name = if native_repeat_mode(&mode) {
                    runtime
                        .playback
                        .get_mut(tag)
                        .expect("animation playback disappeared")
                        .controls[index]
                        .elapsed = 0.0;
                    // sub_100016C50 seeks first, so a selected time-zero
                    // spineEvent is queued before PLAYBACK_REPEAT.
                    apply_native_targets_with_context(runtime, resources, data_root, tag, 2);
                    "PLAYBACK_REPEAT"
                } else if mode == "once" {
                    "PLAYBACK_END"
                } else {
                    ""
                };
                queue_animation_event(
                    runtime,
                    tag,
                    AnimationTimelineEvent {
                        name: event_name.to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    },
                );
            } else {
                let control = &mut runtime
                    .playback
                    .get_mut(tag)
                    .expect("animation playback disappeared")
                    .controls[index];
                control.playing = false;
                control.finished_pending_removal = true;
            }
        }
        index += 1;
    }
    apply_native_targets_with_context(runtime, resources, data_root, tag, 3);
}

#[cfg(test)]
pub(in super::super) fn install_update(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
) -> LuaResult<()> {
    install_update_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        None,
        None,
    )
}

pub(in super::super) fn install_update_with_resources(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
) -> LuaResult<()> {
    install_update_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        Some(resource_runtime),
        Some(data_root),
    )
}

fn install_update_inner(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime;
    let sprite_data_root = data_root;
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "update",
        lua.create_function(move |_, args: MultiValue| {
            let delta_time = f64::from(native_required_number(&args, 0, "update")? as f32);
            let event_groups;
            {
                let resources = resources
                    .as_ref()
                    .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                let tags = runtime.playback.keys().cloned().collect::<Vec<_>>();
                for tag in tags {
                    if let (Some(resources), Some(data_root)) =
                        (resources.as_deref(), sprite_data_root.as_deref())
                    {
                        advance_native_scene_with_resources(
                            &mut runtime,
                            resources,
                            data_root,
                            &tag,
                            delta_time,
                        );
                    } else {
                        advance_native_scene(&mut runtime, &tag, delta_time);
                    }
                }

                // sub_100016FE4 drains a snapshot of each component's event
                // vector. Callback-generated events are appended to the now
                // empty live queue and wait for the next wrapper update.
                event_groups = std::mem::take(&mut runtime.pending_event_tags)
                    .into_iter()
                    .filter_map(|tag| {
                        runtime
                            .pending_events
                            .remove(&tag)
                            .map(|events| (tag, events))
                    })
                    .collect::<Vec<_>>();
            }
            for (tag, events) in event_groups {
                for event in events {
                    let action = runtime
                        .lock()
                        .expect("animation runtime lock poisoned")
                        .playback
                        .get(&tag)
                        .map(|playback| playback.current_action.clone())
                        .unwrap_or_default();
                    if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                        eprintln!("animation-native event tag={tag} event={}", event.name);
                    }
                    if let Value::Function(callback) = callbacks.raw_get::<Value>(tag.as_str())? {
                        // sub_100016FE4 pushes exactly six values in this
                        // order and reads the component's current action at
                        // dispatch time rather than when the event was queued.
                        callback.call::<()>((
                            tag.clone(),
                            action,
                            event.name,
                            event.integer,
                            event.number,
                            event.text,
                        ))?;
                    }
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(speed: f64) -> AnimationControl {
        AnimationControl {
            action: "action".to_owned(),
            elapsed: 7.25,
            previous_elapsed: 7.25,
            duration: 2.0,
            speed,
            paused: false,
            playing: true,
            callback_installed: true,
            finished_pending_removal: false,
        }
    }

    fn playback(mode: &str, speed: f64) -> AnimationPlayback {
        AnimationPlayback::active("action".to_owned(), mode.to_owned(), 7.25, 2.0, speed)
    }

    fn test_region(name: &str) -> SpriteCatalogRegion {
        SpriteCatalogRegion {
            native_sheet_id: 1,
            texture_source: "timeline-test.pvr".to_owned(),
            sprite: stella_assets::ka3d::SpriteRegion {
                name: name.to_owned(),
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                pivot_x: 0,
                pivot_y: 0,
                atlas_rotation: 0,
            },
        }
    }

    #[test]
    fn unchanged_newer_discrete_state_blocks_an_older_state_change() {
        let direct_target = |sprite: Vec<(f64, String)>| AnimationTarget {
            sprite,
            sprite_kind: AnimationSpriteTrackKind::DirectSprite,
            ..AnimationTarget::default()
        };
        let definition = AnimationDefinition {
            actions: BTreeMap::from([
                (
                    "lower".to_owned(),
                    AnimationAction {
                        targets: BTreeMap::from([(
                            "SLOT".to_owned(),
                            direct_target(vec![
                                (0.0, "LOWER_0".to_owned()),
                                (0.5, "LOWER_1".to_owned()),
                            ]),
                        )]),
                        ..AnimationAction::default()
                    },
                ),
                (
                    "upper".to_owned(),
                    AnimationAction {
                        targets: BTreeMap::from([(
                            "SLOT".to_owned(),
                            direct_target(vec![(0.0, "UPPER".to_owned())]),
                        )]),
                        ..AnimationAction::default()
                    },
                ),
            ]),
            slots: vec!["SLOT".to_owned()],
            ..AnimationDefinition::default()
        };
        let mut playback =
            AnimationPlayback::active("lower".to_owned(), "repeat".to_owned(), 0.25, 1.0, 1.0);
        playback.controls.push(AnimationControl {
            action: "upper".to_owned(),
            elapsed: 0.25,
            previous_elapsed: 0.25,
            duration: 1.0,
            speed: 1.0,
            paused: false,
            playing: true,
            callback_installed: true,
            finished_pending_removal: false,
        });
        let mut runtime = AnimationRuntime::default();
        runtime.definitions.insert("scene".to_owned(), definition);
        runtime.playback.insert("scene".to_owned(), playback);
        runtime.sprite_regions.insert(
            "scene".to_owned(),
            ["LOWER_0", "LOWER_1", "UPPER"]
                .into_iter()
                .map(|name| (name.to_owned(), test_region(name)))
                .collect(),
        );

        apply_native_targets(&mut runtime, "scene", 4);
        assert_eq!(
            runtime.playback["scene"].latched_targets["SLOT"]
                .bound_sprite
                .as_ref()
                .map(|binding| binding.sprite.as_str()),
            Some("UPPER")
        );

        let playback = runtime.playback.get_mut("scene").unwrap();
        playback.controls[0].previous_elapsed = 0.25;
        playback.controls[0].elapsed = 0.75;
        apply_native_targets(&mut runtime, "scene", 3);
        assert_eq!(
            runtime.playback["scene"].latched_targets["SLOT"]
                .bound_sprite
                .as_ref()
                .map(|binding| binding.sprite.as_str()),
            Some("UPPER"),
            "the last State owns the usage even when its key index is unchanged"
        );
    }

    #[test]
    fn unchanged_newer_z_order_state_blocks_an_older_state_change() {
        let z_target = |z_order: Vec<(f64, i64)>| AnimationTarget {
            z_order,
            ..AnimationTarget::default()
        };
        let definition = AnimationDefinition {
            actions: BTreeMap::from([
                (
                    "lower".to_owned(),
                    AnimationAction {
                        targets: BTreeMap::from([(
                            "SLOT".to_owned(),
                            z_target(vec![(0.0, 10), (0.5, 20)]),
                        )]),
                        ..AnimationAction::default()
                    },
                ),
                (
                    "upper".to_owned(),
                    AnimationAction {
                        targets: BTreeMap::from([("SLOT".to_owned(), z_target(vec![(0.0, 30)]))]),
                        ..AnimationAction::default()
                    },
                ),
            ]),
            slots: vec!["SLOT".to_owned()],
            ..AnimationDefinition::default()
        };
        let mut playback =
            AnimationPlayback::active("lower".to_owned(), "repeat".to_owned(), 0.25, 1.0, 1.0);
        playback.controls.push(AnimationControl {
            action: "upper".to_owned(),
            elapsed: 0.25,
            previous_elapsed: 0.25,
            duration: 1.0,
            speed: 1.0,
            paused: false,
            playing: true,
            callback_installed: true,
            finished_pending_removal: false,
        });
        let mut runtime = AnimationRuntime::default();
        runtime.definitions.insert("scene".to_owned(), definition);
        runtime.playback.insert("scene".to_owned(), playback);

        apply_native_targets(&mut runtime, "scene", 4);
        assert_eq!(
            runtime.playback["scene"].latched_targets["SLOT"].z_order,
            30
        );

        let playback = runtime.playback.get_mut("scene").unwrap();
        playback.controls[0].previous_elapsed = 0.25;
        playback.controls[0].elapsed = 0.75;
        apply_native_targets(&mut runtime, "scene", 3);
        assert_eq!(
            runtime.playback["scene"].latched_targets["SLOT"].z_order, 30,
            "the last DiscreteInt State owns zOrder even when its key index is unchanged"
        );
    }

    #[test]
    fn native_completion_modes_seek_repeat_and_only_end_literal_once() {
        for mode in ["", "repeat"] {
            let mut runtime = AnimationRuntime::default();
            let mut value = playback(mode, 1.0);
            value.controls[0].elapsed = 0.0;
            runtime.playback.insert("scene".to_owned(), value);
            advance_native_scene(&mut runtime, "scene", 3.0);
            let control = &runtime.playback["scene"].controls[0];
            assert_eq!(control.elapsed, 0.0);
            assert!(control.playing);
            assert_eq!(runtime.pending_events["scene"][0].name, "PLAYBACK_REPEAT");
        }

        for (mode, expected) in [("once", "PLAYBACK_END"), ("unexpected", "")] {
            let mut runtime = AnimationRuntime::default();
            let mut value = playback(mode, 1.0);
            value.controls[0].elapsed = 0.0;
            runtime.playback.insert("scene".to_owned(), value);
            advance_native_scene(&mut runtime, "scene", 3.0);
            let control = &runtime.playback["scene"].controls[0];
            assert_eq!(control.elapsed, control.duration);
            assert!(control.playing);
            assert_eq!(runtime.pending_events["scene"][0].name, expected);
        }
    }

    #[test]
    fn native_once_state_matches_float32_upper_boundary_rules() {
        let mut value = control(1.0);
        value.elapsed = 0.0;
        assert_eq!(advance_native_once(&value, 0.5), (0.5, false));
        assert_eq!(advance_native_once(&value, 3.0), (2.0, true));

        value.elapsed = value.duration;
        assert_eq!(advance_native_once(&value, 1.0), (2.0, false));

        value.duration = 0.0;
        value.elapsed = 0.0;
        assert_eq!(advance_native_once(&value, 1.0), (0.0, false));

        value.duration = 2.0;
        value.elapsed = 3.0;
        assert_eq!(advance_native_once(&value, 0.25), (2.75, false));
        assert_eq!(advance_native_once(&value, 1.0), (2.0, true));

        value.elapsed = 0.5;
        value.speed = -1.0;
        assert_eq!(advance_native_once(&value, 1.0), (-0.5, false));
    }

    #[test]
    fn every_control_completion_reads_the_latest_shared_wrapper_mode() {
        let mut runtime = AnimationRuntime::default();
        let mut value =
            AnimationPlayback::active("older".to_owned(), "once".to_owned(), 0.0, 1.0, 1.0);
        value.controls.push(AnimationControl {
            action: "newer".to_owned(),
            elapsed: 0.0,
            previous_elapsed: 0.0,
            duration: 10.0,
            speed: 1.0,
            paused: false,
            playing: true,
            callback_installed: true,
            finished_pending_removal: false,
        });
        value.current_action = "newer".to_owned();
        runtime.playback.insert("scene".to_owned(), value);

        advance_native_scene(&mut runtime, "scene", 2.0);

        assert_eq!(runtime.playback["scene"].controls[0].elapsed, 1.0);
        assert_eq!(runtime.playback["scene"].controls[1].elapsed, 2.0);
        assert_eq!(runtime.pending_events["scene"][0].name, "PLAYBACK_END");
    }

    #[test]
    fn native_repeat_discards_large_delta_overshoot_before_next_cycle() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let callbacks = lua.create_table().unwrap();
        let observed = lua.create_table().unwrap();
        let callback_events = observed.clone();
        callbacks
            .set(
                "scene",
                lua.create_function(
                    move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                        callback_events.raw_set(callback_events.raw_len() + 1, event)
                    },
                )
                .unwrap(),
            )
            .unwrap();

        let action = AnimationAction {
            event_track: vec![
                (
                    0.0,
                    Some(AnimationTimelineEvent {
                        name: "zero".to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    }),
                ),
                (
                    0.5,
                    Some(AnimationTimelineEvent {
                        name: "middle".to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    }),
                ),
            ],
            ..AnimationAction::default()
        };
        let mut runtime = AnimationRuntime::default();
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([("action".to_owned(), action)]),
                ..AnimationDefinition::default()
            },
        );
        let mut repeating = playback("", 1.0);
        repeating.controls[0].elapsed = 0.0;
        repeating.controls[0].previous_elapsed = 0.0;
        runtime.playback.insert("scene".to_owned(), repeating);
        queue_animation_event(
            &mut runtime,
            "scene",
            AnimationTimelineEvent {
                name: "zero".to_owned(),
                integer: 0,
                number: 0.0,
                text: String::new(),
            },
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();
        let update = animation_native.get::<mlua::Function>("update").unwrap();

        update.call::<()>(3.25).unwrap();
        {
            let runtime = runtime.lock().unwrap();
            let playback = &runtime.playback["scene"];
            assert_eq!(playback.controls[0].elapsed, 0.0);
            assert!(playback.controls[0].playing);
        }
        assert_eq!(observed.raw_len(), 3);
        assert_eq!(observed.raw_get::<String>(1).unwrap(), "zero");
        assert_eq!(observed.raw_get::<String>(2).unwrap(), "zero");
        assert_eq!(observed.raw_get::<String>(3).unwrap(), "PLAYBACK_REPEAT");

        update.call::<()>(0.1).unwrap();
        assert_eq!(observed.raw_len(), 3);
    }

    #[test]
    fn zero_duration_action_stays_active_without_completion_events() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let callbacks = lua.create_table().unwrap();
        let observed = lua.create_table().unwrap();
        let callback_events = observed.clone();
        callbacks
            .set(
                "scene",
                lua.create_function(
                    move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                        callback_events.raw_set(callback_events.raw_len() + 1, event)
                    },
                )
                .unwrap(),
            )
            .unwrap();

        let mut runtime = AnimationRuntime::default();
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([("action".to_owned(), AnimationAction::default())]),
                ..AnimationDefinition::default()
            },
        );
        let mut static_action = playback("repeat", 1.0);
        static_action.controls[0].elapsed = 0.0;
        static_action.controls[0].previous_elapsed = 0.0;
        static_action.controls[0].duration = 0.0;
        runtime.playback.insert("scene".to_owned(), static_action);
        let runtime = Arc::new(Mutex::new(runtime));
        install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();
        let update = animation_native.get::<mlua::Function>("update").unwrap();

        update.call::<()>(1.0).unwrap();
        update.call::<()>(1.0).unwrap();

        let runtime = runtime.lock().unwrap();
        let playback = &runtime.playback["scene"];
        assert_eq!(playback.controls[0].elapsed, 0.0);
        assert!(playback.controls[0].playing);
        assert_eq!(observed.raw_len(), 0);
    }

    #[test]
    fn callback_queued_seek_event_waits_for_the_next_native_update() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let callbacks = lua.create_table().unwrap();
        let observed = lua.create_table().unwrap();
        let callback_events = observed.clone();
        let callback_native = animation_native.clone();
        callbacks
            .set(
                "scene",
                lua.create_function(
                    move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                        callback_events.raw_set(callback_events.raw_len() + 1, event.clone())?;
                        if event == "zero" {
                            callback_native
                                .get::<mlua::Function>("seek")?
                                .call::<()>(("scene", 0.5))?;
                        }
                        Ok(())
                    },
                )
                .unwrap(),
            )
            .unwrap();

        let action = AnimationAction {
            event_track: vec![
                (
                    0.0,
                    Some(AnimationTimelineEvent {
                        name: "zero".to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    }),
                ),
                (
                    0.5,
                    Some(AnimationTimelineEvent {
                        name: "middle".to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    }),
                ),
            ],
            ..AnimationAction::default()
        };
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            BTreeMap::from([("action".to_owned(), 1.0)]),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([("action".to_owned(), action)]),
                ..AnimationDefinition::default()
            },
        );
        let mut retained = playback("once", 1.0);
        retained.controls[0].elapsed = 0.0;
        retained.controls[0].previous_elapsed = 0.0;
        retained.controls[0].paused = true;
        runtime.playback.insert("scene".to_owned(), retained);
        queue_animation_event(
            &mut runtime,
            "scene",
            AnimationTimelineEvent {
                name: "zero".to_owned(),
                integer: 0,
                number: 0.0,
                text: String::new(),
            },
        );

        let runtime = Arc::new(Mutex::new(runtime));
        super::super::controls::install_controls(&lua, &animation_native, Arc::clone(&runtime))
            .unwrap();
        install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();
        let update = animation_native.get::<mlua::Function>("update").unwrap();

        update.call::<()>(0.0).unwrap();
        assert_eq!(observed.raw_len(), 1);
        assert_eq!(observed.raw_get::<String>(1).unwrap(), "zero");
        assert_eq!(
            runtime.lock().unwrap().playback["scene"].controls[0].elapsed,
            0.5
        );

        update.call::<()>(0.0).unwrap();
        assert_eq!(observed.raw_len(), 2);
        assert_eq!(observed.raw_get::<String>(2).unwrap(), "middle");
    }
}
