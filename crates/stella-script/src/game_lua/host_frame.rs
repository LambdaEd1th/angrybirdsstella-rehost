//! Native frame update (`sub_10005E898`) and draw dispatcher (`sub_10004BAB4`).

mod body_export;

use super::StellaLua;
use crate::*;
use body_export::NativeBodyLuaState;

impl StellaLua {
    fn advance_native_aim_stream(&self, delta_seconds: f64) {
        // 0x10006064C rereads GameLua+0x6A8 after both Lua update and the
        // particle-system call. A locked frame does not age, compact, or
        // spawn AimStream particles.
        if !self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .physics_enabled
        {
            return;
        }
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        if bridge.physics_enabled {
            bridge.update_native_aim_stream(delta_seconds as f32);
        }
    }

    /// Run the per-frame callback recovered from the native update loop.
    pub fn update(&self, delta_seconds: f64) -> Result<bool, ScriptError> {
        // AppController hands GameLua an `S0` value. `sub_10005E898` retains
        // that float as the raw delta, then performs the time-multiplier FMUL
        // in single precision before forwarding `float,float` to Lua. Keep
        // the public host API convenient for clocks while closing that native
        // numeric boundary immediately on entry.
        let raw_delta = delta_seconds as f32;
        let delta_seconds = f64::from(raw_delta);
        self.recover_native_audio_output()?;
        publish_native_key_state(&self.lua)?;
        let environment = game_environment(&self.lua)?;
        // sub_10005E898 converts Lua's g_safeToQuit with lua_toboolean and
        // stores GameLua+0x6AC before applyUserZoom, touch publication and the
        // later script update callback.
        let safe_to_quit = !matches!(
            environment.get::<Value>("g_safeToQuit")?,
            Value::Nil | Value::Boolean(false)
        );
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .safe_to_quit = safe_to_quit;
        self.advance_input_zoom(delta_seconds)?;
        self.publish_touches()?;
        // sub_10005E898 passes two frame deltas to Lua: the engine-scaled
        // value first (v238), followed by the original wall-clock delta
        // (v230). The second value is not elapsed game time.
        let multiplier = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .delta_time_multiplier;
        let scaled_delta = f64::from(multiplier * raw_delta);
        // The first GameLua+0x6A8 read at 0x10005ECDC gates one complete
        // native phase: both ThemeManager layers, ThemeSpriteData, the fixed
        // Box2D loop, interpolation, and RenderObjectData destruction timers.
        // A lock acquired by updatePhysics does not cancel the remainder of
        // this already-entered phase; a second read below gates scene export.
        let physics_phase_unlocked = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .physics_enabled;
        let needs_theme_world_limits = if physics_phase_unlocked {
            let bridge = self.render.lock().expect("render bridge lock poisoned");
            bridge.native_theme_frame_needs_world_limits(scaled_delta)
        } else {
            false
        };
        let theme_world_limits = if needs_theme_world_limits {
            live_theme_world_limits(&self.lua)?
        } else {
            ThemeWorldLimits::default()
        };
        if physics_phase_unlocked {
            let resources = self
                .resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            bridge.advance_native_theme_frame_with_resources(
                scaled_delta,
                theme_world_limits,
                &resources,
                &self.data_root,
            );
        }
        let physics_update_millis = if physics_phase_unlocked {
            self.step_physics(scaled_delta)?
        } else {
            0.0
        };
        environment.set("g_physicsUpdateMillis", f64::from(physics_update_millis))?;
        if physics_phase_unlocked {
            // RenderObjectData +0x146 is processed after the fixed Box2D loop
            // and interpolation, but before Purple's second physics-lock read.
            // At expiry it writes strength=0 and retains the Lua object in
            // deadBlocks rather than removing the native object immediately.
            let ready_destructions = self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .take_ready_object_destructions(scaled_delta);
            for name in ready_destructions {
                native_queue_dead_block(&self.lua, &name, 0.0)?;
            }
        }

        // The second lock read at 0x10005F23C gates body-to-Lua export,
        // bounce/motion aggregation, joint export and out-of-bound reporting.
        // The locked branch jumps past the setters, retaining the preceding
        // Lua globals rather than overwriting them with false/empty values.
        let (scene_frame, joint_endpoint_exports, rolling_audio_levels) = {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            if !bridge.physics_enabled {
                (None, Vec::new(), [0.0_f32; 3])
            } else {
                let scene_nonempty = !bridge.scene.is_empty();
                // Purple exports and aggregates motion in one ordered scene
                // traversal. Snapshot each body before the shared pass updates
                // its cached previous-awake bit.
                let (body_states, motion) =
                    NativeBodyLuaState::collect_and_advance(&mut bridge, scaled_delta);
                let (has_moving_objects, has_awake_objects, has_moving_zero_tolerance) = motion;
                let joint_endpoint_exports = bridge.native_joint_endpoint_exports();
                let rolling_audio_levels = bridge.native_rolling_audio_levels();
                (
                    Some((
                        has_moving_objects,
                        has_awake_objects,
                        has_moving_zero_tolerance,
                        scene_nonempty,
                        body_states,
                    )),
                    joint_endpoint_exports,
                    rolling_audio_levels,
                )
            }
        };
        if let Some((
            has_moving_objects,
            has_awake_objects,
            has_moving_zero_tolerance,
            scene_nonempty,
            body_states,
        )) = scene_frame
        {
            // 0x10005F278..0x10005F798 exports every body only once per
            // rendered frame, after all fixed steps and interpolation.
            if scene_nonempty {
                // GameLua+0x308 is the ordered scene-map node count. Purple
                // resolves objects.world first, then requires the existing
                // global out-of-bound table without replacing or clearing it.
                let world = object_world(&self.lua)?;
                let out_of_boundaries =
                    environment.get::<mlua::Table>("g_outOfBoundariesObjects")?;
                self.export_native_body_lua_state(&world, &out_of_boundaries, body_states)?;
            }
            environment.set("hasMovingObjects", has_moving_objects)?;
            environment.set("hasAwakeObjects", has_awake_objects)?;
            environment.set("hasMovingObjectsZeroTolerance", has_moving_zero_tolerance)?;
        }
        // 0x10005F944..0x10005FA98 reads both b2Joint anchors before looking
        // up the Lua descriptor. Body-local/coordType-two descriptors are
        // still looked up but deliberately retain their authored fields.
        if !joint_endpoint_exports.is_empty() {
            let objects = native_lua_object(&self.lua, NativeLuaObject::Objects)?
                .ok_or_else(|| LuaError::RuntimeError("objects is not a table".to_owned()))?;
            let joints: mlua::Table = objects.get("joints")?;
            for joint in joint_endpoint_exports {
                let descriptor: mlua::Table = joints.get(joint.name)?;
                if joint.coord_type == 2 {
                    continue;
                }
                descriptor.set("x1", f64::from(joint.first.0))?;
                descriptor.set("y1", f64::from(joint.first.1))?;
                descriptor.set("x2", f64::from(joint.second.0))?;
                descriptor.set("y2", f64::from(joint.second.1))?;
            }
        }
        // 0x10005FA98..0x10005FD58 updates/stops the three material loops
        // after body/joint export and before installed-app delivery/Lua update.
        self.update_native_rolling_audio(rolling_audio_levels)?;
        let trace_input = std::env::var_os("STELLA_TRACE_INPUT").is_some();
        if trace_input {
            trace_input_tables(&environment, "before-update")?;
        }
        let trace_calls = std::env::var_os("STELLA_TRACE_LUA_CALLS").is_some();
        if trace_calls {
            self.lua.set_hook(HookTriggers::ON_CALLS, |_, debug| {
                let names = debug.names();
                let source = debug.source();
                eprintln!(
                    "lua-call {} [{}] {}",
                    names.name.as_deref().unwrap_or("?"),
                    names.name_what.unwrap_or("?"),
                    source.short_src.as_deref().unwrap_or("?")
                );
                Ok(VmState::Continue)
            })?;
        }
        match environment.get::<Value>("update")? {
            Value::Function(function) => {
                let result = function.call::<()>((scaled_delta, delta_seconds));
                if trace_calls {
                    self.lua.remove_hook();
                }
                result?;
                // The particle-system virtual call is at 0x1000605D0, after
                // Lua `update(scaled, raw)`. Particles spawned by that callback
                // therefore receive this same frame's integration. Its W2
                // argument is a fresh physics-lock comparison.
                {
                    let resources = self
                        .resource_runtime
                        .lock()
                        .expect("resource runtime lock poisoned");
                    self.render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .update_particles_with_resources(
                            scaled_delta,
                            delta_seconds,
                            &resources,
                            &self.data_root,
                        );
                }
                // `0x1000605D4..0x10006064C` walks GameLua+0x3F0 backwards
                // only after Lua update and the Particles virtual call.
                self.render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .drain_pending_native_joint_destructions();
                // sub_10005E898 reloads its original float32 frame argument
                // and updates AimStream after Lua and particles. It is neither
                // time-multiplied nor advanced while physics is locked.
                self.advance_native_aim_stream(delta_seconds);
                if std::env::var_os("STELLA_TRACE_CAMERA").is_some()
                    && let Value::Table(camera) = environment.get::<Value>("gameCamera")?
                {
                    let elapsed = environment.get::<f64>("time").unwrap_or(0.0);
                    eprintln!(
                        "camera-state elapsed={elapsed:.6} delta={delta_seconds:.6} scaled_delta={scaled_delta:.6} spring={:?} spring_no_target={:?} slider={:?} target={:?} scale1={:?} scale2={:?} zoom={:?}",
                        environment.get::<Value>("g_cameraSpringForceConstant")?,
                        environment.get::<Value>("g_cameraSpringForceConstantNoTarget")?,
                        camera.get::<Value>("cameraAnimationSlider")?,
                        camera.get::<Value>("cameraAnimationSliderTarget")?,
                        camera.get::<Value>("animationWorldScale")?,
                        camera.get::<Value>("animationWorldScale2")?,
                        camera.get::<Value>("currentZoomedScale")?,
                    );
                }
                if trace_input {
                    trace_input_tables(&environment, "after-update")?;
                }
                clear_input_edges(&self.lua)?;
                self.finish_mouse_wheel_frame()?;
                Ok(true)
            }
            _ => {
                if trace_calls {
                    self.lua.remove_hook();
                }
                {
                    let resources = self
                        .resource_runtime
                        .lock()
                        .expect("resource runtime lock poisoned");
                    self.render
                        .lock()
                        .expect("render bridge lock poisoned")
                        .update_particles_with_resources(
                            scaled_delta,
                            delta_seconds,
                            &resources,
                            &self.data_root,
                        );
                }
                self.render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .drain_pending_native_joint_destructions();
                self.advance_native_aim_stream(delta_seconds);
                clear_input_edges(&self.lua)?;
                self.finish_mouse_wheel_frame()?;
                Ok(false)
            }
        }
    }

    /// Invoke the shipped top-level draw callback after resetting the native
    /// command buffers for this frame.
    ///
    /// `sub_10004BAB4` is only the `drawGameNative` member called from
    /// `GameScene:draw`; it must not be promoted into a second host-level
    /// `DrawCalls` pass. The Lua 5.1 `gamelogic.lua` callback owns the outer
    /// order: menu tree, notifications (including the LEAVES transition),
    /// notification particles, loading-screen state machine, subsystems, and
    /// menu particles.
    pub fn draw(&self) -> Result<bool, ScriptError> {
        // sub_10004BAB4 enters the retained GameLua+0x310 scene tree directly.
        // It never scans `objects.world` for deleted names. LevelLoad and the
        // registered removeObject member already own the two native teardown
        // boundaries, while constructor commit handles an explicitly replaced
        // world table before publishing a new RenderObjectData record.
        {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            bridge.commands.clear();
            bridge.text_commands.clear();
            bridge.rect_commands.clear();
            bridge.capture_commands.clear();
            bridge.next_draw_order = 0;
            bridge.z_order_min = f64::NEG_INFINITY;
            bridge.z_order_max = f64::INFINITY;
        }
        let environment = game_environment(&self.lua)?;
        let Value::Function(draw) = environment.get::<Value>("draw")? else {
            return Ok(false);
        };
        draw.call::<()>(())?;
        Ok(true)
    }
}
