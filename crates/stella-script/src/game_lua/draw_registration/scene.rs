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
                let streams = bridge.trajectory_streams.clone();
                let mut commands = Vec::new();
                for stream in streams {
                    if !stream.normal_sprite.is_empty() {
                        let bound_region = resources
                            .active_atlas_catalog_region(&stream.normal_sprite, &data_root);
                        let mut bound_composite =
                            resources.active_bound_composite(&stream.normal_sprite);
                        if bound_region.is_none() && bound_composite.is_none() {
                            bound_composite = Some(Vec::new());
                        }
                        commands.extend(stream.points.into_iter().map(|(x, y)| RenderCommand {
                            order: 0,
                            sprite: stream.normal_sprite.clone(),
                            texture: None,
                            texture_scale: 1.0,
                            masked_texture_binding: None,
                            bound_region: bound_region.clone(),
                            bound_composite: bound_composite.clone(),
                            shader: None,
                            clip_holes: Vec::new(),
                            dirt: None,
                            // sub_10006D9C0 divides each point before the
                            // GL context applies its divided translation.
                            x: f64::from(x as f32 / game_world_scale),
                            y: f64::from(y as f32 / game_world_scale),
                            state: trail_state,
                            world_space: false,
                        }));
                    }
                    if let Some((x, y)) = stream.puff
                        && !stream.special_sprite.is_empty()
                    {
                        let bound_region = resources
                            .active_atlas_catalog_region(&stream.special_sprite, &data_root);
                        let mut bound_composite =
                            resources.active_bound_composite(&stream.special_sprite);
                        if bound_region.is_none() && bound_composite.is_none() {
                            bound_composite = Some(Vec::new());
                        }
                        commands.push(RenderCommand {
                            order: 0,
                            sprite: stream.special_sprite,
                            texture: None,
                            texture_scale: 1.0,
                            masked_texture_binding: None,
                            bound_region,
                            bound_composite,
                            shader: None,
                            clip_holes: Vec::new(),
                            dirt: None,
                            x: f64::from(x as f32 / game_world_scale),
                            y: f64::from(y as f32 / game_world_scale),
                            state: trail_state,
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
                    .scene_draw_object(&name)
                else {
                    continue;
                };
                if !callback_state_object.visible {
                    continue;
                }
                // sub_100528834 retrieves the original Lua object table from
                // its registry reference; callbacks do not receive a name.
                let callback_object = object_world(lua)?.raw_get::<Value>(name.as_str())?;
                let (pre, post) = {
                    let callbacks = draw_callbacks.borrow();
                    (
                        callbacks.pre.get(&name).cloned(),
                        callbacks.post.get(&name).cloned(),
                    )
                };
                let previous = render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .begin_scene_object_draw(&callback_state_object);
                if std::env::var_os("STELLA_TRACE_DRAW_CALLBACKS").is_some()
                    && (pre.is_some() || post.is_some())
                {
                    eprintln!(
                        "draw-callback name={name:?} pre={} post={} object={}",
                        pre.is_some(),
                        post.is_some(),
                        describe_value(&callback_object)
                    );
                }
                if let Some(function) = pre {
                    // sub_10004BAB4 pushes RenderObjectData+0x139, the
                    // horizontal-flip byte, immediately before the callback.
                    function.call::<()>((
                        callback_object.clone(),
                        callback_state_object.horizontal_flip,
                    ))?;
                }
                // The native dispatcher retains only the object pointer while
                // the pre callback runs, then reloads alpha, sprite/composite,
                // scale, transform and decoration fields from that live
                // record. Reacquire the compact draw snapshot here so a Lua
                // visual mutation affects the same submission.
                let Some(object) = render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .scene_draw_object(&name)
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
                        .get(&name)
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
                        runtime.transforms.insert(name.clone(), transform);
                        runtime
                            .matrices
                            .insert(name.clone(), AnimationAffine::from_transform(transform));
                        animation_render_commands(&runtime, &name)
                    };
                    if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
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
                    let mut resources = resource_runtime
                        .lock()
                        .expect("resource runtime lock poisoned");
                    // sub_10006D5B4 reads `shader` from the retained Lua
                    // object after the pre-draw callback, then builds the
                    // shared cached shader through sub_1000222E4/
                    // sub_100529F68. This is the path used by
                    // GoldTransformer's `2d-sprite-gold` table.
                    let shader = match callback_object.clone() {
                        Value::Table(object_table) => {
                            match object_table.raw_get::<Value>("shader")? {
                                Value::Table(shader) => Some(sprite_shader_from_lua(
                                    shader,
                                    &mut resources.shader_cache,
                                )?),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .push_scene_object(&object, &resources, &data_root, shader);
                }
                if let Some(function) = post {
                    let horizontal_flip = render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .scene
                        .get(&name)
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
