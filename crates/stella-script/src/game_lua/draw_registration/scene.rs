//! Z-ordered scene dispatcher (`sub_10004BAB4`) and anchor-ordered trails.

use crate::*;

mod trails;
mod walk;

use trails::push_native_trajectory_streams;
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
    // Purple passes the same static `"shader"` literal to its Lua bridge at
    // 0x10006D6E0 and again to the shader builder at 0x10006D720. Keep one
    // interned Lua key alive with the registered native member so the live
    // per-object raw lookup remains exact without rebuilding that key for
    // every RenderObjectData visit.
    let shader_key = lua.create_string("shader")?;
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
            let (z_bounds, draw_world_scale) = {
                let bridge = render.lock().expect("render bridge lock poisoned");
                (bridge.native_scene_z_bounds(), bridge.world_scale as f32)
            };
            let mut trajectory_drawn = false;
            for (z_bucket, entry) in
                NativeSceneWalk::new(Arc::clone(&render), z_bounds, draw_world_scale)
            {
                let Some((name, visit)) = entry else {
                    if let Some(draw) = z_order_draw.as_ref() {
                        draw.call::<()>(f64::from(z_bucket))?;
                    }
                    continue;
                };
                let callback_state_object = visit.callback;
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
                // The two +0x558 trajectory records are not a scene-wide
                // pre-pass. 0x10004BF28 tests the first visible +0x140
                // controllable or +0x148 level-goal record and inserts them
                // immediately before that object's pre callback.
                if !trajectory_drawn && callback_state_object.trajectory_anchor {
                    let resources = resource_runtime
                        .lock()
                        .expect("resource runtime lock poisoned");
                    let mut bridge = render.lock().expect("render bridge lock poisoned");
                    push_native_trajectory_streams(&mut bridge, &resources, &data_root);
                    trajectory_drawn = true;
                }
                // RenderObjectData+0x20/+0x158/+0x160 are reached directly
                // from the pointer already resolved by the scene-name map.
                // The stable Rust slot models those three inline holders and
                // deliberately avoids a second per-object name-tree search.
                let (callback_object, pre, initial_post) = {
                    let callbacks = draw_callbacks.borrow();
                    let record = callbacks
                        .record(visit.callback_slot)
                        .expect("live scene object must retain its native callback record");
                    (
                        Value::Table(record.object.clone()),
                        record.pre.clone(),
                        record.post.clone(),
                    )
                };
                // RenderObjectData+0x158 is invoked at 0x10004BFA4 before
                // 0x10004BFE0 starts installing this object's GL state. A pre
                // callback therefore observes the persistent context left by
                // the preceding scene/z draw and may mutate the live record.
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
                    Some(visit.initial_draw_object)
                };
                let Some(object) = object else {
                    continue;
                };
                let post_horizontal_flip = object.horizontal_flip;
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
                    let transform = {
                        let mut bridge = render.lock().expect("render bridge lock poisoned");
                        // 0x10004BFE0 installs the callback context and the
                        // custom animation branch consumes it immediately.
                        // Keep those two native operations inside one bridge
                        // acquisition instead of introducing a host-only
                        // synchronization boundary between them.
                        bridge.install_scene_post_draw_state(&object);
                        bridge.flash_animation_transform(&object, &current_action)
                    };
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
                            match object_table.raw_get::<Value>(&shader_key)? {
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
                            .push_scene_object(object, decoration_resources, shader);
                    } else {
                        // Purple's ordinary sprite branch consumes only the
                        // retained RenderObjectData resource pointer. Keep the
                        // overwhelmingly common path free of ResourceRuntime.
                        render
                            .lock()
                            .expect("render bridge lock poisoned")
                            .push_scene_object(object, None, None);
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
                        .record(visit.callback_slot)
                        .and_then(|record| record.post.clone())
                } else {
                    initial_post
                };
                if let Some(function) = post {
                    function.call::<()>((callback_object, post_horizontal_flip))?;
                }
            }
            Ok(())
        })?,
    )
}
