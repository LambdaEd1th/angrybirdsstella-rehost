//! Z-ordered scene dispatcher (`sub_10004BAB4`) and trail pre-pass.

use crate::*;

mod walk;

use walk::NativeSceneWalk;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // Diagnostic switches are process-launch configuration. Resolve them
    // once while installing the native member; querying libc getenv for
    // every RenderObjectData leaf was rehost-only work inside the hottest
    // scene walk and has no counterpart in sub_10004BAB4.
    let trace_draw_callbacks = std::env::var_os("STELLA_TRACE_DRAW_CALLBACKS").is_some();
    let trace_animation = std::env::var_os("STELLA_TRACE_ANIMATION").is_some();
    globals.set(
        "drawGameNative",
        lua.create_function(move |lua, _: MultiValue| {
            if render
                .lock()
                .expect("render bridge lock poisoned")
                .game_rendering_disabled
            {
                return Ok(());
            }
            // sub_10004BAB4 snapshots DrawCalls.hasZOrderedDraws and its
            // `draw` member once before entering the integer-z tree. For each
            // persistent native z bucket it calls draw(z) before visiting the
            // bucket's SpriteSheet/name leaves. Empty nodes are retained by
            // removal. The outer frame later calls draw(nil) as the final
            // sentinel for callbacks above all bodies.
            let z_order_draw = {
                let environment = game_environment(lua)?;
                match environment.get::<Value>("DrawCalls")? {
                    Value::Table(draw_calls)
                        if !matches!(
                            draw_calls.get::<Value>("hasZOrderedDraws")?,
                            Value::Nil | Value::Boolean(false)
                        ) =>
                    {
                        match draw_calls.get::<Value>("draw")? {
                            Value::Function(draw) => Some(draw),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            };
            // sub_10004BAB4 calls sub_10006D9C0 before the scene list. It
            // draws both 0x38-byte trail records in fixed slot order.
            {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                let top_left_x = bridge.top_left_x as f32;
                let top_left_y = bridge.top_left_y as f32;
                let world_scale = bridge.world_scale as f32;
                let game_world_scale = bridge.game_world_scale as f32;
                let trail_state = RenderState {
                    translate_x: f64::from(-top_left_x / game_world_scale),
                    translate_y: f64::from(-top_left_y / game_world_scale),
                    scale_x: f64::from(world_scale * game_world_scale),
                    scale_y: f64::from(world_scale * game_world_scale),
                    ..RenderState::default()
                };
                let mut commands = Vec::new();
                // sub_10006D9C0 reads the two fixed 0x38-byte records at
                // GameLua+0x558 in place and re-reads each point-vector end
                // while iterating. The bridge lock already excludes the Lua
                // mutators, so borrowing here avoids a Rust-only deep clone of
                // both point vectors on every rendered frame.
                for stream in &bridge.trajectory_streams {
                    if !stream.normal_sprite.is_empty() {
                        let bound_region = resources
                            .active_atlas_catalog_region(&stream.normal_sprite, &data_root)
                            .map(Arc::new);
                        let mut bound_composite = resources
                            .active_bound_composite(&stream.normal_sprite)
                            .map(Arc::new);
                        if bound_region.is_none() && bound_composite.is_none() {
                            bound_composite = Some(Arc::new(Vec::new()));
                        }
                        let sprite: SharedSpriteName = stream.normal_sprite.as_str().into();
                        commands.extend(stream.points.iter().map(|&(x, y)| RenderCommand {
                            order: 0,
                            sprite: sprite.clone(),
                            texture: None,
                            bound_region: bound_region.clone(),
                            bound_composite: bound_composite.clone(),
                            geometry: None,
                            shader: None,
                            dirt: None,
                            // sub_10006D9C0 divides each point before the
                            // GL context applies its divided translation.
                            x: x as f32 / game_world_scale,
                            y: y as f32 / game_world_scale,
                            state: trail_state.into(),
                            world_space: false,
                        }));
                    }
                    if let Some((x, y)) = stream.puff
                        && !stream.special_sprite.is_empty()
                    {
                        let bound_region = resources
                            .active_atlas_catalog_region(&stream.special_sprite, &data_root)
                            .map(Arc::new);
                        let mut bound_composite = resources
                            .active_bound_composite(&stream.special_sprite)
                            .map(Arc::new);
                        if bound_region.is_none() && bound_composite.is_none() {
                            bound_composite = Some(Arc::new(Vec::new()));
                        }
                        commands.push(RenderCommand {
                            order: 0,
                            sprite: stream.special_sprite.as_str().into(),
                            texture: None,
                            bound_region,
                            bound_composite,
                            geometry: None,
                            shader: None,
                            dirt: None,
                            x: x as f32 / game_world_scale,
                            y: y as f32 / game_world_scale,
                            state: trail_state.into(),
                            world_space: false,
                        });
                    }
                }
                bridge.extend_render_commands(commands);
            }
            let z_bounds = render
                .lock()
                .expect("render bridge lock poisoned")
                .native_scene_z_bounds();
            for (z_bucket, name) in NativeSceneWalk::new(Arc::clone(&render), z_bounds) {
                let Some(name) = name else {
                    if let Some(draw) = z_order_draw.as_ref() {
                        draw.call::<()>(f64::from(z_bucket))?;
                    }
                    continue;
                };
                let Some(callback_state_object) = render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .scene_callback_object(name.as_ref())
                else {
                    continue;
                };
                let callback_horizontal_flip = callback_state_object.horizontal_flip;
                // sub_10004BAB4 handles rectangular water before resolving
                // either retained Lua callback. In normal gameplay the fill
                // replaces the editor-only placeholder and this visit ends.
                let water_replaces_object = callback_state_object.is_water
                    && !callback_state_object.collision_is_circle
                    && {
                        let mut bridge = render.lock().expect("render bridge lock poisoned");
                        bridge.push_scene_water(&callback_state_object) && !bridge.editing
                    };
                if water_replaces_object {
                    continue;
                }
                // RenderObjectData+0x20 owns the exact table through a Lua
                // registry reference. Keep a compatibility fallback for
                // tests or extension-created scene records that bypass the
                // recovered constructors, but retain its first table too.
                let (callback_object, pre, initial_post) = {
                    let mut callbacks = draw_callbacks.borrow_mut();
                    if let Some(record) = callbacks.records.get(name.as_ref()) {
                        (
                            Value::Table(record.object.clone()),
                            record.pre.clone(),
                            record.post.clone(),
                        )
                    } else {
                        let object = object_world(lua)?.raw_get::<Value>(name.as_ref())?;
                        if let Value::Table(table) = &object {
                            callbacks.records.insert(
                                name.to_string(),
                                DrawCallbackRecord {
                                    object: table.clone(),
                                    pre: None,
                                    post: None,
                                },
                            );
                        }
                        (object, None, None)
                    }
                };
                // RenderObjectData+0x158 is tested before the original starts
                // consuming the visual fields.  A pre callback can mutate all
                // of those fields, so defer its SceneDrawObject snapshot until
                // after Lua returns.  Objects without a pre callback keep the
                // native one-pointer/one-snapshot fast path.
                let (previous, initial_draw_object) = {
                    let mut bridge = render.lock().expect("render bridge lock poisoned");
                    let initial_draw_object = pre
                        .is_none()
                        .then(|| bridge.scene_draw_object(name.as_ref()))
                        .flatten();
                    let previous =
                        bridge.begin_scene_object_draw_callback(callback_state_object);
                    (previous, initial_draw_object)
                };
                if trace_draw_callbacks && (pre.is_some() || initial_post.is_some())
                {
                    eprintln!(
                        "draw-callback name={name:?} pre={} post={} object={}",
                        pre.is_some(),
                        initial_post.is_some(),
                        describe_value(&callback_object)
                    );
                }
                if let Some(function) = pre.as_ref() {
                    // sub_10004BAB4 pushes RenderObjectData+0x139, the
                    // horizontal-flip byte, immediately before the callback.
                    function.call::<()>((
                        callback_object.clone(),
                        callback_horizontal_flip,
                    ))?;
                }
                // The native dispatcher retains only the object pointer while
                // the pre callback runs, then reloads alpha, sprite/composite,
                // scale, transform and decoration fields from that live
                // record. Reacquire the compact draw snapshot here so a Lua
                // visual mutation affects the same submission.
                let object = if pre.is_some() {
                    render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .scene_draw_object(name.as_ref())
                } else {
                    initial_draw_object
                };
                let Some(object) = object
                else {
                    render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .finish_scene_object_draw(previous);
                    continue;
                };
                if object.flash_animation {
                    // Preserve Purple's interpolated position and authored
                    // ability rotations. The recovered BirdAnimation flying
                    // state is the one late writer that derives angle from a
                    // 30 Hz velocity sample, so the renderer also supplies
                    // the matching two-slot visual velocity sample for it.
                    let current_action = animation_runtime
                        .lock()
                        .expect("animation runtime lock poisoned")
                        .playback
                        .get(name.as_ref())
                        .map(|playback| playback.current_action.clone())
                        .unwrap_or_default();
                    let transform = render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .flash_animation_transform(&object, &current_action);
                    let mut commands = {
                        let mut runtime = animation_runtime
                            .lock()
                            .expect("animation runtime lock poisoned");
                        runtime.transforms.insert(name.to_string(), transform);
                        runtime
                            .matrices
                            .insert(name.to_string(), AnimationAffine::from_transform(transform));
                        animation_render_commands(&runtime, name.as_ref())
                    };
                    if trace_animation {
                        let (physics_slot, physics_alpha) = {
                            let bridge = render.lock().expect("render bridge lock poisoned");
                            (
                                bridge.physics_interpolation_slot,
                                bridge.physics_accumulator * f32::from_bits(0x41EF_FFFF),
                            )
                        };
                        let velocities = object.display_interpolation_velocities;
                        eprintln!(
                            "animation-native scene-draw tag={name} action={current_action:?} sprites={} transform=({:.3},{:.3}; {:.3},{:.3}; angle={:.6}) physics=(slot={physics_slot},alpha={physics_alpha:.6},v0={:.6},{:.6},v1={:.6},{:.6})",
                            commands.len(),
                            transform.x,
                            transform.y,
                            transform.scale_x,
                            transform.scale_y,
                            transform.angle,
                            velocities[0].x,
                            velocities[0].y,
                            velocities[1].x,
                            velocities[1].y,
                        );
                    }
                    let mut bridge = render.lock().expect("render bridge lock poisoned");
                    let clip_rect = bridge.state.clip_rect;
                    for command in &mut commands {
                        command.state.clip_rect = clip_rect;
                    }
                    bridge.extend_render_commands(commands);
                } else {
                    // sub_10006D5B4 reads `shader` from the retained Lua
                    // object after the pre-draw callback, then builds the
                    // shared cached shader through sub_1000222E4/
                    // sub_100529F68. This is the path used by
                    // GoldTransformer's `2d-sprite-gold` table.
                    let shader_table = match &callback_object {
                        Value::Table(object_table) => {
                            match object_table.raw_get::<Value>("shader")? {
                                Value::Table(shader) => Some(shader),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let decoration_needs_resources = object.ray.is_none()
                        && object.decoration.as_deref().is_some_and(|decoration| {
                            !decoration.sprite.is_empty() && decoration.amount > 0
                        });
                    if shader_table.is_some() || decoration_needs_resources {
                        let mut resources = resource_runtime
                            .lock()
                            .expect("resource runtime lock poisoned");
                        let shader = shader_table
                            .map(|shader| {
                                sprite_shader_from_lua(shader, &mut resources.shader_cache)
                            })
                            .transpose()?;
                        let decoration_resources = decoration_needs_resources
                            .then_some((&*resources, data_root.as_path()));
                        render
                            .lock()
                            .expect("render bridge lock poisoned")
                            .push_scene_object(&object, decoration_resources, shader);
                    } else {
                        // Purple's ordinary sprite branch consumes only the
                        // retained RenderObjectData resource pointer. Keep the
                        // overwhelmingly common path free of ResourceRuntime.
                        render
                            .lock()
                            .expect("render bridge lock poisoned")
                            .push_scene_object(&object, None, None);
                    }
                }
                // RenderObjectData+0x160 is loaded at 0x10004C300, after the
                // pre callback and the ordinary draw. A pre callback can
                // replace the post holder for this same visit. Without a pre
                // callback there was no intervening Lua execution, so retain
                // the first lookup.
                let post = if pre.is_some() {
                    draw_callbacks
                        .borrow()
                        .records
                        .get(name.as_ref())
                        .and_then(|record| record.post.clone())
                } else {
                    initial_post
                };
                if let Some(function) = post {
                    let horizontal_flip = render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .scene
                        .get(name.as_ref())
                        .map(|object| object.horizontal_flip)
                        .unwrap_or(object.horizontal_flip);
                    function.call::<()>((callback_object, horizontal_flip))?;
                }
                render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .finish_scene_object_draw(previous);
            }
            Ok(())
        })?,
    )
}
