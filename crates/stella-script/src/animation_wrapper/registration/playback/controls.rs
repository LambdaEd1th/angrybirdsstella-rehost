//! Playback state controls registered by `sub_10000EC80`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::*;

fn apply_targets(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    mode: u8,
) {
    if let (Some(resources), Some(data_root)) = (resources, data_root) {
        super::update::apply_native_targets_with_resources(
            runtime, resources, data_root, tag, mode,
        );
    } else {
        super::update::apply_native_targets(runtime, tag, mode);
    }
}

fn advance_scene(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    delta_time: f64,
) {
    if let (Some(resources), Some(data_root)) = (resources, data_root) {
        super::update::advance_native_scene_with_resources(
            runtime, resources, data_root, tag, delta_time,
        );
    } else {
        super::update::advance_native_scene(runtime, tag, delta_time);
    }
}

#[cfg(test)]
pub(in super::super) fn install_controls(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    install_controls_inner(lua, animation_native, animation_runtime, None, None)
}

pub(in super::super) fn install_controls_with_resources(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
) -> LuaResult<()> {
    install_controls_inner(
        lua,
        animation_native,
        animation_runtime,
        Some(resource_runtime),
        Some(data_root),
    )
}

fn install_controls_inner(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "isPlaying",
        lua.create_function(move |_, tag: String| {
            Ok(runtime
                .lock()
                .expect("animation runtime lock poisoned")
                .playback
                .get(&tag)
                .and_then(AnimationPlayback::current_control)
                .is_some_and(|control| control.playing && !control.paused))
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime.clone();
    let sprite_data_root = data_root.clone();
    animation_native.set(
        "start",
        lua.create_function(move |_, (tag, action, mode): (String, String, String)| {
            let resources = resources
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            let known_action = runtime
                .definitions
                .get(&tag)
                .is_some_and(|definition| definition.actions.contains_key(&action));
            if !known_action {
                return Ok(());
            }
            let duration = runtime
                .actions
                .get(&tag)
                .and_then(|actions| actions.get(&action))
                .copied()
                .map(|duration| f64::from(duration as f32))
                .unwrap_or(0.0);
            let playback = runtime
                .playback
                .entry(tag.clone())
                .or_insert_with(|| AnimationPlayback::detached(action.clone(), duration));
            playback.detached_current = None;
            if let Some(index) = playback.active_control_index(&action) {
                // sub_100410A18 resets an existing named control in place.
                // Its speed and installed callback survive, and its vector
                // position (therefore target precedence) does not change.
                let control = &mut playback.controls[index];
                control.elapsed = 0.0;
                control.previous_elapsed = 0.0;
                control.duration = duration;
                control.paused = false;
                control.playing = true;
                control.finished_pending_removal = false;
            } else {
                playback.controls.push(AnimationControl {
                    action: action.clone(),
                    elapsed: 0.0,
                    previous_elapsed: 0.0,
                    duration,
                    speed: 1.0,
                    paused: false,
                    playing: true,
                    callback_installed: false,
                    finished_pending_removal: false,
                });
            }

            // sub_100012F18 starts the native control, performs the hidden
            // float32 0.00001-second update and a mode-4 forced application,
            // then overwrites the wrapper's shared action/mode fields and
            // installs the completion callback.
            advance_scene(
                &mut runtime,
                resources.as_deref(),
                sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                &tag,
                f64::from(0.00001_f32),
            );
            apply_targets(
                &mut runtime,
                resources.as_deref(),
                sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                &tag,
                4,
            );
            let playback = runtime
                .playback
                .get_mut(&tag)
                .expect("newly started animation playback disappeared");
            playback.current_action = action.clone();
            playback.mode = mode.clone();
            if let Some(index) = playback.active_control_index(&action) {
                playback.controls[index].callback_installed = true;
            }
            if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                eprintln!(
                    "animation-native start tag={tag} action={action} mode={mode} duration={duration:.4} controls={}",
                    playback.controls.len()
                );
            }
            // sub_10001CAF0 returns zero Lua results.
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime.clone();
    let sprite_data_root = data_root.clone();
    animation_native.set(
        "stop",
        lua.create_function(move |_, (tag, action): (String, String)| {
            let resources = resources
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            if action.is_empty() {
                let control_count = runtime
                    .playback
                    .get(&tag)
                    .map_or(0, |playback| playback.controls.len());
                for index in 0..control_count {
                    if let Some(control) = runtime
                        .playback
                        .get_mut(&tag)
                        .and_then(|playback| playback.controls.get_mut(index))
                    {
                        control.elapsed = 0.0;
                        control.previous_elapsed = 0.0;
                        control.playing = false;
                        control.paused = false;
                        control.finished_pending_removal = false;
                    }
                    apply_targets(
                        &mut runtime,
                        resources.as_deref(),
                        sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                        &tag,
                        2,
                    );
                }
            } else {
                let (found, removed_current) =
                    runtime
                        .playback
                        .get_mut(&tag)
                        .map_or((false, None), |playback| {
                            let Some(index) = playback.active_control_index(&action) else {
                                return (false, None);
                            };
                            let mut control = playback.controls.swap_remove(index);
                            control.elapsed = 0.0;
                            control.previous_elapsed = 0.0;
                            control.playing = false;
                            control.paused = false;
                            control.finished_pending_removal = false;
                            let current =
                                (control.action == playback.current_action).then_some(control);
                            (true, current)
                        });
                if let Some(control) = removed_current
                    && let Some(playback) = runtime.playback.get_mut(&tag)
                {
                    playback.detached_current = Some(control);
                }
                if found {
                    apply_targets(
                        &mut runtime,
                        resources.as_deref(),
                        sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                        &tag,
                        2,
                    );
                }
            }
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime.clone();
    let sprite_data_root = data_root.clone();
    animation_native.set(
        "stopAll",
        lua.create_function(move |_, ()| {
            let resources = resources
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            let tags = runtime.playback.keys().cloned().collect::<Vec<_>>();
            for tag in tags {
                let control_count = runtime.playback[&tag].controls.len();
                for index in 0..control_count {
                    let control = &mut runtime
                        .playback
                        .get_mut(&tag)
                        .expect("animation playback disappeared")
                        .controls[index];
                    control.elapsed = 0.0;
                    control.previous_elapsed = 0.0;
                    control.playing = false;
                    control.paused = false;
                    control.finished_pending_removal = false;
                    apply_targets(
                        &mut runtime,
                        resources.as_deref(),
                        sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                        &tag,
                        2,
                    );
                }
            }
            Ok(())
        })?,
    )?;
    for (method, paused) in [("pause", true), ("resume", false)] {
        let runtime = Arc::clone(&animation_runtime);
        animation_native.set(
            method,
            lua.create_function(move |_, tag: String| {
                if let Some(playback) = runtime
                    .lock()
                    .expect("animation runtime lock poisoned")
                    .playback
                    .get_mut(&tag)
                    && let Some(control) = playback.current_control_mut()
                {
                    control.paused = paused;
                    control.playing = !paused;
                    if !paused {
                        // sub_100013B9C restores state 3 even when the tag's
                        // retained control was previously stopped/detached.
                        control.finished_pending_removal = false;
                    }
                }
                Ok(())
            })?,
        )?;
    }
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "setSpeed",
        lua.create_function(move |_, args: MultiValue| {
            // setSpeed and seek share generated adapter sub_10001C8C0:
            // slot 1 is a strict string and slot 2 a strict NUMBER.
            let tag = native_required_string(&args, 0, "setSpeed")?;
            let speed = f64::from(native_required_number(&args, 1, "setSpeed")? as f32);
            if let Some(playback) = runtime
                .lock()
                .expect("animation runtime lock poisoned")
                .playback
                .get_mut(&tag)
                && let Some(control) = playback.current_control_mut()
            {
                // sub_100013D08 stores the float verbatim, including negative
                // values used for reverse playback.
                control.speed = speed;
            }
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime;
    let sprite_data_root = data_root;
    animation_native.set(
        "seek",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "seek")?;
            let time = f64::from(native_required_number(&args, 1, "seek")? as f32);
            let resources = resources
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            let found = if let Some(control) = runtime
                .playback
                .get_mut(&tag)
                .and_then(AnimationPlayback::current_control_mut)
            {
                // sub_10001396C resolves the scene and sub_10040E798 stores
                // the adapter's float32 seek time verbatim before a mode-2
                // forced application of every discrete state.
                control.elapsed = time;
                true
            } else {
                false
            };
            if found {
                apply_targets(
                    &mut runtime,
                    resources.as_deref(),
                    sprite_data_root.as_deref().map(std::path::PathBuf::as_path),
                    &tag,
                    2,
                );
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_preserves_native_zero_duration_and_float32_duration() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            BTreeMap::from([
                ("static".to_owned(), 0.0),
                ("moving".to_owned(), 0.123_456_789),
            ]),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([
                    ("static".to_owned(), AnimationAction::default()),
                    ("moving".to_owned(), AnimationAction::default()),
                ]),
                ..AnimationDefinition::default()
            },
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_controls(&lua, &animation_native, Arc::clone(&runtime)).unwrap();
        let start = animation_native.get::<mlua::Function>("start").unwrap();

        start.call::<()>(("scene", "static", "once")).unwrap();
        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .current_control()
                .unwrap()
                .duration,
            0.0
        );

        start.call::<()>(("scene", "moving", "once")).unwrap();
        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .current_control()
                .unwrap()
                .duration,
            f64::from(0.123_456_79_f32)
        );
        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .current_control()
                .unwrap()
                .elapsed,
            f64::from(0.00001_f32)
        );
    }

    #[test]
    fn native_active_controls_preserve_order_speed_and_per_property_fallback() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let mut base = AnimationAction::default();
        base.targets
            .entry("root".to_owned())
            .or_default()
            .translation
            .push((0.0, [10.0, 20.0]));
        let mut overlay = AnimationAction::default();
        overlay
            .targets
            .entry("root".to_owned())
            .or_default()
            .rotation
            .push((0.0, 0.5));
        let mut newest = AnimationAction::default();
        newest
            .targets
            .entry("root".to_owned())
            .or_default()
            .translation
            .push((0.0, [30.0, 40.0]));
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            BTreeMap::from([
                ("base".to_owned(), 1.0),
                ("overlay".to_owned(), 1.0),
                ("newest".to_owned(), 1.0),
            ]),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([
                    ("base".to_owned(), base),
                    ("overlay".to_owned(), overlay),
                    ("newest".to_owned(), newest),
                ]),
                ..AnimationDefinition::default()
            },
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_controls(&lua, &animation_native, Arc::clone(&runtime)).unwrap();
        let start = animation_native.get::<mlua::Function>("start").unwrap();
        let set_speed = animation_native.get::<mlua::Function>("setSpeed").unwrap();
        let stop = animation_native.get::<mlua::Function>("stop").unwrap();

        start.call::<()>(("scene", "base", "repeat")).unwrap();
        set_speed.call::<()>(("scene", 0.25)).unwrap();
        start.call::<()>(("scene", "overlay", "once")).unwrap();
        start.call::<()>(("scene", "base", "once")).unwrap();
        {
            let runtime = runtime.lock().unwrap();
            let playback = &runtime.playback["scene"];
            assert_eq!(
                playback
                    .controls
                    .iter()
                    .map(|control| control.action.as_str())
                    .collect::<Vec<_>>(),
                ["base", "overlay"]
            );
            assert_eq!(playback.controls[0].speed, 0.25);
            let local = animation_entity_local_transform(&runtime, "scene", "root").unwrap();
            assert_eq!((local.x, local.y, local.angle), (10.0, 20.0, 0.5));
        }

        start.call::<()>(("scene", "newest", "once")).unwrap();
        {
            let runtime = runtime.lock().unwrap();
            let local = animation_entity_local_transform(&runtime, "scene", "root").unwrap();
            assert_eq!((local.x, local.y, local.angle), (30.0, 40.0, 0.5));
        }
        stop.call::<()>(("scene", "newest")).unwrap();
        let runtime = runtime.lock().unwrap();
        let local = animation_entity_local_transform(&runtime, "scene", "root").unwrap();
        assert_eq!((local.x, local.y, local.angle), (10.0, 20.0, 0.5));
    }

    #[test]
    fn removing_the_last_property_state_keeps_the_component_latched_value() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("root".to_owned())
            .or_default()
            .translation = vec![(0.0, [0.0, 10.0]), (1.0, [100.0, 30.0])];
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite = vec![(0.0, "FIRST".to_owned()), (0.5, "SECOND".to_owned())];
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            BTreeMap::from([("moving".to_owned(), 1.0)]),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([("moving".to_owned(), action)]),
                slots: vec!["SLOT_BODY".to_owned()],
                ..AnimationDefinition::default()
            },
        );
        runtime.playback.insert(
            "scene".to_owned(),
            AnimationPlayback::detached("moving".to_owned(), 1.0),
        );
        runtime.sprite_regions.insert(
            "scene".to_owned(),
            ["FIRST", "SECOND"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        SpriteCatalogRegion {
                            native_sheet_id: 1,
                            texture_source: "test-animation.pvr".to_owned(),
                            sprite: stella_assets::ka3d::SpriteRegion {
                                name: name.to_owned(),
                                x: 0,
                                y: 0,
                                width: 0,
                                height: 0,
                                pivot_x: 0,
                                pivot_y: 0,
                                atlas_rotation: 0,
                            },
                        },
                    )
                })
                .collect(),
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_controls(&lua, &animation_native, Arc::clone(&runtime)).unwrap();
        let start = animation_native.get::<mlua::Function>("start").unwrap();
        let seek = animation_native.get::<mlua::Function>("seek").unwrap();
        let stop = animation_native.get::<mlua::Function>("stop").unwrap();

        {
            let runtime = runtime.lock().unwrap();
            let local = animation_entity_local_transform(&runtime, "scene", "root").unwrap();
            assert_eq!((local.x, local.y), (0.0, 0.0));
            assert!(animation_render_commands(&runtime, "scene").is_empty());
        }
        start.call::<()>(("scene", "moving", "once")).unwrap();
        seek.call::<()>(("scene", 0.5)).unwrap();
        stop.call::<()>(("scene", "moving")).unwrap();

        let runtime = runtime.lock().unwrap();
        assert!(runtime.playback["scene"].controls.is_empty());
        let local = animation_entity_local_transform(&runtime, "scene", "root").unwrap();
        assert_eq!((local.x, local.y), (50.0, 20.0));
        assert_eq!(
            animation_render_commands(&runtime, "scene")[0].sprite,
            "SECOND"
        );
    }

    #[test]
    fn native_control_state_survives_completion_and_distinguishes_stop_kinds() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            BTreeMap::from([("action".to_owned(), 1.0)]),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions: BTreeMap::from([("action".to_owned(), AnimationAction::default())]),
                ..AnimationDefinition::default()
            },
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_controls(&lua, &animation_native, Arc::clone(&runtime)).unwrap();
        super::super::update::install_update(
            &lua,
            &animation_native,
            Arc::clone(&runtime),
            lua.create_table().unwrap(),
        )
        .unwrap();

        let start = animation_native.get::<mlua::Function>("start").unwrap();
        let update = animation_native.get::<mlua::Function>("update").unwrap();
        let is_playing = animation_native.get::<mlua::Function>("isPlaying").unwrap();
        let pause = animation_native.get::<mlua::Function>("pause").unwrap();
        let resume = animation_native.get::<mlua::Function>("resume").unwrap();
        let stop = animation_native.get::<mlua::Function>("stop").unwrap();
        let stop_all = animation_native.get::<mlua::Function>("stopAll").unwrap();

        start.call::<()>(("scene", "action", "once")).unwrap();
        update.call::<()>(1.0).unwrap();
        assert!(is_playing.call::<bool>("scene").unwrap());
        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .current_control()
                .unwrap()
                .elapsed,
            1.0
        );

        pause.call::<()>("scene").unwrap();
        assert!(!is_playing.call::<bool>("scene").unwrap());
        resume.call::<()>("scene").unwrap();
        assert!(is_playing.call::<bool>("scene").unwrap());

        stop.call::<()>(("scene", "")).unwrap();
        {
            let runtime = runtime.lock().unwrap();
            assert_eq!(runtime.playback["scene"].controls.len(), 1);
            assert!(!runtime.playback["scene"].controls[0].playing);
        }
        resume.call::<()>("scene").unwrap();
        update.call::<()>(0.25).unwrap();
        assert_eq!(
            runtime.lock().unwrap().playback["scene"].controls[0].elapsed,
            0.25
        );

        stop.call::<()>(("scene", "action")).unwrap();
        {
            let runtime = runtime.lock().unwrap();
            assert!(runtime.playback["scene"].controls.is_empty());
            assert!(
                !runtime.playback["scene"]
                    .detached_current
                    .as_ref()
                    .unwrap()
                    .playing
            );
        }
        resume.call::<()>("scene").unwrap();
        assert!(is_playing.call::<bool>("scene").unwrap());
        update.call::<()>(0.25).unwrap();
        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .detached_current
                .as_ref()
                .unwrap()
                .elapsed,
            0.0
        );

        stop_all.call::<()>(()).unwrap();
        assert!(is_playing.call::<bool>("scene").unwrap());
    }
}
