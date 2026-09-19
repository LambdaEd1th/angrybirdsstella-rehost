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

fn timeline_event(name: &str) -> AnimationTimelineEvent {
    AnimationTimelineEvent {
        name: name.to_owned(),
        integer: 0,
        number: 0.0,
        text: String::new(),
    }
}

fn populate_close_state(runtime: &mut AnimationRuntime, tag: &str) {
    runtime
        .actions
        .insert(tag.to_owned(), BTreeMap::from([("action".to_owned(), 2.0)]));
    runtime
        .definitions
        .insert(tag.to_owned(), AnimationDefinition::default());
    runtime
        .transforms
        .insert(tag.to_owned(), AnimationTransform::default());
    runtime
        .matrices
        .insert(tag.to_owned(), AnimationAffine::default());
    runtime.descendant_reflections.insert(tag.to_owned(), false);
    let mut retained = playback("once", 1.0);
    retained.controls[0].paused = true;
    runtime.playback.insert(tag.to_owned(), retained);
    runtime.skins.insert(tag.to_owned(), "default".to_owned());
    runtime
        .shaders
        .insert(tag.to_owned(), SpriteShader::default());
    runtime
        .sprite_geometry
        .insert(tag.to_owned(), BTreeMap::new());
    runtime
        .sprite_metrics
        .insert(tag.to_owned(), BTreeMap::new());
    runtime
        .sprite_regions
        .insert(tag.to_owned(), BTreeMap::new());
}

fn assert_close_state_removed(runtime: &AnimationRuntime, tag: &str) {
    assert!(!runtime.actions.contains_key(tag));
    assert!(!runtime.definitions.contains_key(tag));
    assert!(!runtime.transforms.contains_key(tag));
    assert!(!runtime.matrices.contains_key(tag));
    assert!(!runtime.descendant_reflections.contains_key(tag));
    assert!(!runtime.playback.contains_key(tag));
    assert!(!runtime.skins.contains_key(tag));
    assert!(!runtime.shaders.contains_key(tag));
    assert!(!runtime.sprite_geometry.contains_key(tag));
    assert!(!runtime.sprite_metrics.contains_key(tag));
    assert!(!runtime.sprite_regions.contains_key(tag));
}

fn test_region(name: &str) -> SpriteCatalogRegion {
    SpriteCatalogRegion {
        decoded_image: None,
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
    let mut value = AnimationPlayback::active("older".to_owned(), "once".to_owned(), 0.0, 1.0, 1.0);
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

#[test]
fn callback_close_is_deferred_until_every_event_in_its_snapshot_runs() {
    let lua = Lua::new();
    let animation_native = lua.create_table().unwrap();
    let callbacks = lua.create_table().unwrap();
    let observed = lua.create_table().unwrap();
    let callback_events = observed.clone();
    let callback_native = animation_native.clone();
    let mut state = AnimationRuntime::default();
    populate_close_state(&mut state, "scene");
    queue_animation_event(&mut state, "scene", timeline_event("first"));
    queue_animation_event(&mut state, "scene", timeline_event("second"));
    let runtime = Arc::new(Mutex::new(state));
    let callback_runtime = Arc::clone(&runtime);
    callbacks
        .set(
            "scene",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    callback_events.raw_set(callback_events.raw_len() + 1, event.clone())?;
                    if event == "first" {
                        let close = callback_native.get::<mlua::Function>("close")?;
                        close.call::<()>("scene")?;
                        close.call::<()>("scene")?;
                        let runtime = callback_runtime
                            .lock()
                            .expect("animation runtime lock poisoned");
                        callback_events
                            .set("alive_after_close", runtime.playback.contains_key("scene"))?;
                        callback_events.set("deferred_count", runtime.deferred_close_tags.len())?;
                    }
                    Ok(())
                },
            )
            .unwrap(),
        )
        .unwrap();

    super::super::super::resources::install_closing(
        &lua,
        &animation_native,
        Arc::clone(&runtime),
        callbacks.clone(),
    )
    .unwrap();
    install_update(
        &lua,
        &animation_native,
        Arc::clone(&runtime),
        callbacks.clone(),
    )
    .unwrap();

    animation_native
        .get::<mlua::Function>("update")
        .unwrap()
        .call::<()>(0.0)
        .unwrap();

    assert_eq!(observed.raw_len(), 2);
    assert_eq!(observed.raw_get::<String>(1).unwrap(), "first");
    assert_eq!(observed.raw_get::<String>(2).unwrap(), "second");
    assert!(observed.get::<bool>("alive_after_close").unwrap());
    assert_eq!(observed.get::<usize>("deferred_count").unwrap(), 1);
    let runtime = runtime.lock().unwrap();
    assert!(!runtime.dispatching_events);
    assert!(runtime.deferred_close_tags.is_empty());
    assert_close_state_removed(&runtime, "scene");
    drop(runtime);
    assert!(matches!(
        callbacks.raw_get::<Value>("scene").unwrap(),
        Value::Nil
    ));
}

#[test]
fn callback_can_close_a_later_tag_without_suppressing_its_snapshot() {
    let lua = Lua::new();
    let animation_native = lua.create_table().unwrap();
    let callbacks = lua.create_table().unwrap();
    let observed = lua.create_table().unwrap();
    let first_events = observed.clone();
    let first_native = animation_native.clone();
    callbacks
        .set(
            "first",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    first_events.raw_set(first_events.raw_len() + 1, event)?;
                    first_native
                        .get::<mlua::Function>("close")?
                        .call::<()>("second")
                },
            )
            .unwrap(),
        )
        .unwrap();
    let second_events = observed.clone();
    callbacks
        .set(
            "second",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    second_events.raw_set(second_events.raw_len() + 1, event)
                },
            )
            .unwrap(),
        )
        .unwrap();

    let mut state = AnimationRuntime::default();
    populate_close_state(&mut state, "first");
    populate_close_state(&mut state, "second");
    queue_animation_event(&mut state, "first", timeline_event("first-event"));
    queue_animation_event(&mut state, "second", timeline_event("second-event"));
    let runtime = Arc::new(Mutex::new(state));
    super::super::super::resources::install_closing(
        &lua,
        &animation_native,
        Arc::clone(&runtime),
        callbacks.clone(),
    )
    .unwrap();
    install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();

    animation_native
        .get::<mlua::Function>("update")
        .unwrap()
        .call::<()>(0.0)
        .unwrap();

    assert_eq!(observed.raw_len(), 2);
    assert_eq!(observed.raw_get::<String>(1).unwrap(), "first-event");
    assert_eq!(observed.raw_get::<String>(2).unwrap(), "second-event");
    let runtime = runtime.lock().unwrap();
    assert!(runtime.playback.contains_key("first"));
    assert_close_state_removed(&runtime, "second");
}

#[test]
fn nested_update_retains_outer_callbacks_after_draining_a_close() {
    let lua = Lua::new();
    let animation_native = lua.create_table().unwrap();
    let callbacks = lua.create_table().unwrap();
    let observed = lua.create_table().unwrap();

    let mut state = AnimationRuntime::default();
    for tag in ["outer", "inner", "victim"] {
        populate_close_state(&mut state, tag);
    }
    queue_animation_event(&mut state, "outer", timeline_event("outer-event"));
    queue_animation_event(&mut state, "victim", timeline_event("victim-event"));
    let runtime = Arc::new(Mutex::new(state));

    let outer_events = observed.clone();
    let outer_native = animation_native.clone();
    let outer_runtime = Arc::clone(&runtime);
    callbacks
        .set(
            "outer",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    outer_events.raw_set(outer_events.raw_len() + 1, event)?;
                    queue_animation_event(
                        &mut outer_runtime
                            .lock()
                            .expect("animation runtime lock poisoned"),
                        "inner",
                        timeline_event("inner-event"),
                    );
                    outer_native
                        .get::<mlua::Function>("update")?
                        .call::<()>(0.0)
                },
            )
            .unwrap(),
        )
        .unwrap();
    let inner_events = observed.clone();
    let inner_native = animation_native.clone();
    callbacks
        .set(
            "inner",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    inner_events.raw_set(inner_events.raw_len() + 1, event)?;
                    inner_native
                        .get::<mlua::Function>("close")?
                        .call::<()>("victim")
                },
            )
            .unwrap(),
        )
        .unwrap();
    let victim_events = observed.clone();
    callbacks
        .set(
            "victim",
            lua.create_function(
                move |_, (_, _, event, _, _, _): (String, String, String, i32, f64, String)| {
                    victim_events.raw_set(victim_events.raw_len() + 1, event)
                },
            )
            .unwrap(),
        )
        .unwrap();

    super::super::super::resources::install_closing(
        &lua,
        &animation_native,
        Arc::clone(&runtime),
        callbacks.clone(),
    )
    .unwrap();
    install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();

    animation_native
        .get::<mlua::Function>("update")
        .unwrap()
        .call::<()>(0.0)
        .unwrap();

    assert_eq!(observed.raw_len(), 3);
    assert_eq!(observed.raw_get::<String>(1).unwrap(), "outer-event");
    assert_eq!(observed.raw_get::<String>(2).unwrap(), "inner-event");
    assert_eq!(observed.raw_get::<String>(3).unwrap(), "victim-event");
    let runtime = runtime.lock().unwrap();
    assert!(!runtime.dispatching_events);
    assert!(runtime.deferred_close_tags.is_empty());
    assert_close_state_removed(&runtime, "victim");
}

#[test]
fn callback_error_leaves_native_dispatch_flag_and_close_queue_intact() {
    let lua = Lua::new();
    let animation_native = lua.create_table().unwrap();
    let callbacks = lua.create_table().unwrap();
    let callback_native = animation_native.clone();
    callbacks
        .set(
            "scene",
            lua.create_function(
                move |_, _: (String, String, String, i32, f64, String)| -> LuaResult<()> {
                    callback_native
                        .get::<mlua::Function>("close")?
                        .call::<()>("scene")?;
                    Err(mlua::Error::RuntimeError("callback failure".to_owned()))
                },
            )
            .unwrap(),
        )
        .unwrap();

    let mut state = AnimationRuntime::default();
    populate_close_state(&mut state, "scene");
    queue_animation_event(&mut state, "scene", timeline_event("event"));
    let runtime = Arc::new(Mutex::new(state));
    super::super::super::resources::install_closing(
        &lua,
        &animation_native,
        Arc::clone(&runtime),
        callbacks.clone(),
    )
    .unwrap();
    install_update(&lua, &animation_native, Arc::clone(&runtime), callbacks).unwrap();

    assert!(
        animation_native
            .get::<mlua::Function>("update")
            .unwrap()
            .call::<()>(0.0)
            .is_err()
    );
    animation_native
        .get::<mlua::Function>("close")
        .unwrap()
        .call::<()>("scene")
        .unwrap();

    let runtime = runtime.lock().unwrap();
    assert!(runtime.dispatching_events);
    assert_eq!(runtime.deferred_close_tags, ["scene"]);
    assert!(runtime.playback.contains_key("scene"));
}
