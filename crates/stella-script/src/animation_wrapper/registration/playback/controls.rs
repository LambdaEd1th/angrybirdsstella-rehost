//! Playback state controls registered by `sub_10000EC80`.

mod helpers;
mod start;
mod state;
mod stop;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table};

use crate::*;

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
    // Preserve the exact publication order recovered from sub_10000EC80.
    state::install_is_playing(lua, animation_native, Arc::clone(&animation_runtime))?;
    start::install_start(
        lua,
        animation_native,
        Arc::clone(&animation_runtime),
        resource_runtime.clone(),
        data_root.clone(),
    )?;
    stop::install_stop(
        lua,
        animation_native,
        Arc::clone(&animation_runtime),
        resource_runtime.clone(),
        data_root.clone(),
    )?;
    stop::install_stop_all(
        lua,
        animation_native,
        Arc::clone(&animation_runtime),
        resource_runtime.clone(),
        data_root.clone(),
    )?;
    state::install_pause_resume(lua, animation_native, Arc::clone(&animation_runtime))?;
    state::install_set_speed(lua, animation_native, Arc::clone(&animation_runtime))?;
    state::install_seek(
        lua,
        animation_native,
        animation_runtime,
        resource_runtime,
        data_root,
    )
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
    fn named_stop_uses_the_native_swap_with_last_removal_order() {
        let lua = Lua::new();
        let animation_native = lua.create_table().unwrap();
        let actions = ["base", "overlay", "newest"]
            .into_iter()
            .map(|name| (name.to_owned(), AnimationAction::default()))
            .collect::<BTreeMap<_, _>>();
        let mut runtime = AnimationRuntime::default();
        runtime.actions.insert(
            "scene".to_owned(),
            actions.keys().map(|name| (name.clone(), 1.0)).collect(),
        );
        runtime.definitions.insert(
            "scene".to_owned(),
            AnimationDefinition {
                actions,
                ..AnimationDefinition::default()
            },
        );
        let runtime = Arc::new(Mutex::new(runtime));
        install_controls(&lua, &animation_native, Arc::clone(&runtime)).unwrap();
        let start = animation_native.get::<mlua::Function>("start").unwrap();
        let stop = animation_native.get::<mlua::Function>("stop").unwrap();

        for action in ["base", "overlay", "newest"] {
            start.call::<()>(("scene", action, "once")).unwrap();
        }
        stop.call::<()>(("scene", "base")).unwrap();

        assert_eq!(
            runtime.lock().unwrap().playback["scene"]
                .controls
                .iter()
                .map(|control| control.action.as_str())
                .collect::<Vec<_>>(),
            ["newest", "overlay"]
        );
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
