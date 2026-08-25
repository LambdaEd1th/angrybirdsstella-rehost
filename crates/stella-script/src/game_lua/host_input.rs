//! Desktop pointer injection into Purple's compact native key buffers.

use super::StellaLua;
use crate::*;

#[derive(Clone, Copy, Debug)]
struct NativePinchBaseline {
    active: bool,
    initial_distance: f32,
    initial_scale: f32,
}

const EMPTY_NATIVE_PINCH_BASELINE: NativePinchBaseline = NativePinchBaseline {
    active: false,
    initial_distance: 0.0,
    initial_scale: 0.0,
};

#[cfg(not(test))]
static NATIVE_PINCH_BASELINE: Mutex<NativePinchBaseline> = Mutex::new(EMPTY_NATIVE_PINCH_BASELINE);

// The shipped process owns one GameApp, so Purple's three static pinch fields
// cannot be touched concurrently by independent runtimes. Unit tests do create
// many GameApps on parallel test threads, however. Give each simulated process
// thread its own copy while preserving the native cross-runtime lifetime within
// a test thread.
#[cfg(test)]
thread_local! {
    static NATIVE_PINCH_BASELINE: std::cell::RefCell<NativePinchBaseline> =
        const { std::cell::RefCell::new(EMPTY_NATIVE_PINCH_BASELINE) };
}

#[cfg(not(test))]
fn with_native_pinch_baseline<T>(callback: impl FnOnce(&mut NativePinchBaseline) -> T) -> T {
    let mut baseline = NATIVE_PINCH_BASELINE
        .lock()
        .expect("native pinch baseline lock poisoned");
    callback(&mut baseline)
}

#[cfg(test)]
fn with_native_pinch_baseline<T>(callback: impl FnOnce(&mut NativePinchBaseline) -> T) -> T {
    NATIVE_PINCH_BASELINE.with(|baseline| callback(&mut baseline.borrow_mut()))
}

#[cfg(test)]
static NATIVE_PINCH_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn lock_native_pinch_for_test() -> std::sync::MutexGuard<'static, ()> {
    let guard = NATIVE_PINCH_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_native_pinch_baseline(|baseline| *baseline = EMPTY_NATIVE_PINCH_BASELINE);
    guard
}

impl StellaLua {
    /// Inject one of the five names published by Purple's native per-frame
    /// key loop. The platform byte arrays distinguish held state from the two
    /// one-frame edges; repeated down events therefore do not create another
    /// press edge.
    pub fn set_key(&self, key_name: &str, down: bool) -> Result<(), ScriptError> {
        let environment = game_environment(&self.lua)?;
        let key = Value::String(self.lua.create_string(key_name)?);
        let was_down = native_lua_object(&self.lua, NativeLuaObject::KeyHold)?
            .and_then(|table| table.raw_get::<bool>(key.clone()).ok())
            .unwrap_or(false);
        for name in ["keyHold", "g_keyHold", "g_keyHoldNotBlocked"] {
            set_input_flag(&self.lua, &environment, name, key.clone(), down)?;
        }
        if down && !was_down {
            for name in ["keyPressed", "g_keyPressed", "g_keyPressedNotBlocked"] {
                set_input_flag(&self.lua, &environment, name, key.clone(), true)?;
            }
        }
        if !down && was_down {
            for name in ["keyReleased", "g_keyReleased", "g_keyReleasedNotBlocked"] {
                set_input_flag(&self.lua, &environment, name, key.clone(), true)?;
            }
        }
        Ok(())
    }

    /// Publish the active platform touches using Purple's `TouchEvent` table
    /// bridge. Coordinates have already passed the platform's truncating
    /// drawable-pixel conversion; only the first two active touches are
    /// visible to Lua.
    pub fn set_touches(&self, touches: &[(u64, i32, i32)]) -> Result<(), ScriptError> {
        *self.touches.lock().expect("touch-state lock poisoned") = touches.to_vec();
        Ok(())
    }

    /// Reproduce `MyEAGLViewController::viewDidDisappear:`: clear the native
    /// touch vector first, then release LBUTTON only when the controller still
    /// owns its primary pointer. Other held keys are deliberately untouched.
    pub fn view_did_disappear(&self, had_primary_pointer: bool) -> Result<(), ScriptError> {
        self.set_touches(&[])?;
        if !had_primary_pointer {
            return Ok(());
        }
        let cursor = native_lua_object(&self.lua, NativeLuaObject::Cursor)?
            .ok_or_else(|| runtime_error("cursor is not a table"))?;
        let x = cursor.get::<f64>("x").unwrap_or(0.0);
        let y = cursor.get::<f64>("y").unwrap_or(0.0);
        self.set_cursor(x, y, false)
    }

    pub(super) fn publish_touches(&self) -> Result<(), ScriptError> {
        let touches = self
            .touches
            .lock()
            .expect("touch-state lock poisoned")
            .clone();
        let published = self.lua.create_table()?;
        for &(id, x, y) in touches.iter().take(2) {
            let touch = self.lua.create_table()?;
            touch.set("x", f64::from(x))?;
            touch.set("y", f64::from(y))?;
            // Purple stores the UITouch pointer as a 64-bit id but formats the
            // Lua key with `%d`, so only its signed low 32 bits are observed.
            published.set((id as u32 as i32).to_string(), touch)?;
        }
        let count = touches.len().min(2) as f64;
        let environment = game_environment(&self.lua)?;
        environment.set("touches", published.clone())?;
        environment.set("touchcount", count)?;
        self.lua.globals().set("touches", published)?;
        self.lua.globals().set("touchcount", count)?;
        Ok(())
    }

    /// Feed the framework wheel callback recovered at `sub_100029FF8`.
    /// The platform adapter has already converted its native scroll unit to
    /// an integer. Shift selects Purple's fine step; Control suppresses only
    /// the non-smoothed camera change.
    pub fn mouse_wheel(
        &self,
        wheel_delta: i32,
        shift_held: bool,
        control_held: bool,
    ) -> Result<(), ScriptError> {
        {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            let smooth_zooming = bridge.smooth_zooming;
            let game_world_scale = bridge.game_world_scale as f32;
            let max_world_scale = bridge.max_world_scale as f32;
            let zoom = &mut bridge.input_zoom;
            zoom.wheel_pending = true;
            if smooth_zooming {
                let current = zoom.current;
                zoom.previous = current;
                let base_step = if current >= 1.0 { 0.2_f64 } else { 0.1_f64 };
                let mut step = (base_step / f64::from(game_world_scale)) as f32;
                if shift_held {
                    step *= 0.05_f32;
                }
                let wheel = wheel_delta as f32;
                if zoom.smooth_elapsed <= -1.0 || current <= 0.6_f32 || current >= max_world_scale {
                    zoom.smooth_target = wheel.mul_add(step, current);
                    zoom.smooth_start = current;
                    zoom.smooth_elapsed = 0.0;
                    zoom.smooth_duration = 0.5;
                } else {
                    zoom.smooth_target = wheel.mul_add(step * 0.5_f32, zoom.smooth_target);
                    zoom.smooth_start = current;
                    zoom.smooth_duration = 1.0_f32 - zoom.smooth_elapsed;
                    zoom.smooth_elapsed = 0.0;
                }
            } else if !control_held {
                let current = zoom.current;
                zoom.previous = current;
                let base_step = if current >= 1.0 { 0.2_f64 } else { 0.1_f64 };
                let mut step = (base_step / f64::from(game_world_scale)) as f32;
                if shift_held {
                    step *= 0.05_f32;
                }
                if wheel_delta != 0 {
                    zoom.current = (wheel_delta as f32).mul_add(step, current);
                }
            }
        }

        let cursor = native_lua_object(&self.lua, NativeLuaObject::Cursor)?
            .ok_or_else(|| runtime_error("cursor is not a table"))?;
        cursor.set("wheel", f64::from(wheel_delta))?;
        cursor.set("wheelTriggered", true)?;
        Ok(())
    }

    pub(super) fn advance_input_zoom(&self, delta_seconds: f64) -> Result<(), ScriptError> {
        let touches = self
            .touches
            .lock()
            .expect("touch-state lock poisoned")
            .clone();
        let zoom_delta = {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            let smooth_zooming = bridge.smooth_zooming;
            let world_scale = bridge.world_scale as f32;
            let zoom = &mut bridge.input_zoom;

            if smooth_zooming && zoom.smooth_elapsed > -1.0_f32 {
                let frame_delta = (delta_seconds as f32).min(0.1_f32);
                zoom.smooth_elapsed += frame_delta;
                let difference = zoom.smooth_target - zoom.smooth_start;
                let shifted = zoom.smooth_elapsed / zoom.smooth_duration - 1.0_f32;
                let square = shifted * shifted;
                let eased = shifted.mul_add(square, 1.0_f32);
                zoom.current = difference.mul_add(eased, zoom.smooth_start);
                if zoom.smooth_elapsed > zoom.smooth_duration {
                    zoom.smooth_elapsed = -1.0;
                    zoom.current = zoom.smooth_target;
                }
            }

            if touches.len() == 2 {
                let dx = (touches[0].1 as f32) - (touches[1].1 as f32);
                let dy = (touches[0].2 as f32) - (touches[1].2 as f32);
                let distance = dx.mul_add(dx, dy * dy).sqrt();
                with_native_pinch_baseline(|pinch| {
                    if !pinch.active {
                        pinch.active = true;
                        pinch.initial_distance = distance;
                        pinch.initial_scale = world_scale;
                        zoom.current = world_scale;
                    }
                    if pinch.initial_distance > f32::MIN_POSITIVE
                        && pinch.initial_distance < f32::MAX
                    {
                        zoom.previous = zoom.current;
                        zoom.current = pinch.initial_scale * (distance / pinch.initial_distance);
                    }
                });
            } else {
                with_native_pinch_baseline(|pinch| {
                    if pinch.active {
                        pinch.active = false;
                        zoom.previous = zoom.current;
                    }
                });
            }

            (zoom.current != zoom.previous).then_some((zoom.current - zoom.previous) * 0.5_f32)
        };

        if let Some(delta) = zoom_delta {
            let environment = game_environment(&self.lua)?;
            if let Value::Function(callback) = environment.get::<Value>("applyUserZoom")? {
                callback.call::<()>(f64::from(delta))?;
            }
            // sub_10005E898 reloads +0x4FC after the Lua callback, so a
            // callback-side reset is captured in the previous-frame slot.
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            bridge.input_zoom.previous = bridge.input_zoom.current;
        }
        Ok(())
    }

    pub(super) fn finish_mouse_wheel_frame(&self) -> Result<(), ScriptError> {
        let pending = {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            std::mem::take(&mut bridge.input_zoom.wheel_pending)
        };
        if pending {
            let cursor = native_lua_object(&self.lua, NativeLuaObject::Cursor)?
                .ok_or_else(|| runtime_error("cursor is not a table"))?;
            cursor.set("wheelTriggered", false)?;
        }
        Ok(())
    }

    pub fn set_cursor(&self, x: f64, y: f64, down: bool) -> Result<(), ScriptError> {
        let cursor = native_lua_object(&self.lua, NativeLuaObject::Cursor)?
            .ok_or_else(|| runtime_error("cursor is not a table"))?;
        let was_down = cursor.get::<bool>("down").unwrap_or(false);
        cursor.set("x", x)?;
        cursor.set("y", y)?;
        cursor.set("down", down)?;
        let environment = game_environment(&self.lua)?;
        let key = match environment.get::<Value>("LBUTTON")? {
            Value::Nil => Value::String(self.lua.create_string("LBUTTON")?),
            value => value,
        };
        for name in ["keyHold", "g_keyHold", "g_keyHoldNotBlocked"] {
            set_input_flag(&self.lua, &environment, name, key.clone(), down)?;
        }
        if down && !was_down {
            for name in ["keyPressed", "g_keyPressed", "g_keyPressedNotBlocked"] {
                set_input_flag(&self.lua, &environment, name, key.clone(), true)?;
            }
        }
        if !down && was_down {
            for name in ["keyReleased", "g_keyReleased", "g_keyReleasedNotBlocked"] {
                set_input_flag(&self.lua, &environment, name, key.clone(), true)?;
            }
        }
        if std::env::var_os("STELLA_TRACE_UI_INPUT").is_some() && down && !was_down {
            self.execute_source(
                r##"
                local function tracePointerFrame(frame, label)
                    if not frame or frame.__stellaPointerTrace then return end
                    frame.__stellaPointerTrace = true
                    local original = frame.onPointerEvent
                    frame.onPointerEvent = function(self, ...)
                        local values = {...}
                        local results = {original(self, ...)}
                        print(
                            "ui-frame-pointer", label,
                            tostring(values[1]), tostring(values[2]), tostring(values[3]),
                            "result=" .. tostring(results[1]),
                            "target=" .. tostring(results[2])
                        )
                        return unpack(results)
                    end
                end
                tracePointerFrame(notificationsFrame, "notifications")
                tracePointerFrame(menuManager.currentRoot, "root")
                "##,
            )?;
        }
        if std::env::var_os("STELLA_TRACE_INPUT").is_some() {
            let pressed = match environment.get::<Value>("isKeyPressed")? {
                Value::Function(function) => function.call::<bool>(key.clone()).unwrap_or(false),
                _ => false,
            };
            let held = match environment.get::<Value>("isKeyHold")? {
                Value::Function(function) => function.call::<bool>(key).unwrap_or(false),
                _ => false,
            };
            eprintln!(
                "cursor ({x:.1}, {y:.1}) down={down} was_down={was_down} pressed={pressed} held={held}"
            );
        }
        Ok(())
    }
}
