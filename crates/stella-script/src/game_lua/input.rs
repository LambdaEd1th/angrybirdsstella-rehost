//! Per-frame keyboard and pointer edge buffers exposed by `GameLua`.
//!
//! Purple keeps these queries next to its native GameLua lifecycle bridge: the
//! public key maps and the compact `g_*` event arrays are deliberately not the
//! same representation.

use crate::*;

pub(crate) const NATIVE_FRAME_KEYS: [&str; 5] = [
    "LBUTTON",
    "KEY_BACK",
    "KEY_MENU",
    "VOLUME_UP",
    "VOLUME_DOWN",
];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeKeyBuffers {
    pressed: [bool; NATIVE_FRAME_KEYS.len()],
    released: [bool; NATIVE_FRAME_KEYS.len()],
    held: [bool; NATIVE_FRAME_KEYS.len()],
}

impl NativeKeyBuffers {
    pub(crate) fn set(&mut self, key_name: &str, down: bool) -> Option<bool> {
        let index = NATIVE_FRAME_KEYS
            .iter()
            .position(|candidate| *candidate == key_name)?;
        let was_down = self.held[index];
        self.held[index] = down;
        if down && !was_down {
            self.pressed[index] = true;
        } else if !down && was_down {
            self.released[index] = true;
        }
        Some(was_down)
    }

    pub(crate) fn clear_holds(&mut self) {
        self.held.fill(false);
    }

    fn take_frame(&mut self) -> NativeKeyFrame {
        let frame = NativeKeyFrame {
            pressed: self.pressed,
            released: self.released,
            held: self.held,
        };
        self.pressed.fill(false);
        self.released.fill(false);
        frame
    }
}

#[derive(Clone, Copy, Debug)]
struct NativeKeyFrame {
    pressed: [bool; NATIVE_FRAME_KEYS.len()],
    released: [bool; NATIVE_FRAME_KEYS.len()],
    held: [bool; NATIVE_FRAME_KEYS.len()],
}

/// `sub_1000293C8` writes all five entries on every frame, including false
/// entries for keys whose platform bytes are clear, then clears the two edge
/// byte arrays before entering GameLua::update. The published Lua values are
/// deliberately left in place until the following frame overwrites them.
pub(crate) fn publish_native_key_state(
    lua: &Lua,
    buffers: &Mutex<NativeKeyBuffers>,
) -> LuaResult<()> {
    let frame = buffers
        .lock()
        .expect("native key-buffer lock poisoned")
        .take_frame();
    for (object, values) in [
        (NativeLuaObject::KeyPressed, frame.pressed),
        (NativeLuaObject::KeyReleased, frame.released),
        (NativeLuaObject::KeyHold, frame.held),
    ] {
        let Some(table) = native_lua_object(lua, object)? else {
            continue;
        };
        for (key, value) in NATIVE_FRAME_KEYS.into_iter().zip(values) {
            table.raw_set(key, value)?;
        }
    }
    Ok(())
}

pub(crate) fn trace_input_tables(environment: &mlua::Table, phase: &str) -> LuaResult<()> {
    let read = |name: &str| -> (bool, bool, bool) {
        let Ok(Value::Table(table)) = environment.get::<Value>(name) else {
            return (false, false, false);
        };
        (
            table.get::<bool>("LBUTTON").unwrap_or(false),
            table.get::<bool>(1).unwrap_or(false),
            table.raw_len() > 0,
        )
    };
    let key_pressed = read("keyPressed");
    let global_pressed = read("g_keyPressed");
    let key_released = read("keyReleased");
    let global_released = read("g_keyReleased");
    if key_pressed.0 || global_pressed.0 || key_released.0 || global_released.0 {
        eprintln!(
            "input-tables {phase} keyPressed={key_pressed:?} g_keyPressed={global_pressed:?} keyReleased={key_released:?} g_keyReleased={global_released:?}"
        );
    }
    Ok(())
}

pub(crate) fn install_input_queries(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for (function_name, table_name) in [
        ("isKeyPressed", "g_keyPressed"),
        ("isKeyHold", "g_keyHold"),
        ("isKeyReleased", "g_keyReleased"),
    ] {
        environment.set(
            function_name,
            lua.create_function(move |lua, key: Value| {
                let environment = game_environment(lua)?;
                let Value::Table(table) = environment.get::<Value>(table_name)? else {
                    return Ok(false);
                };
                Ok(match table.raw_get::<Value>(key)? {
                    Value::Nil => false,
                    Value::Boolean(value) => value,
                    Value::Integer(value) => value != 0,
                    Value::Number(value) => value != 0.0,
                    _ => true,
                })
            })?,
        )?;
    }
    Ok(())
}
