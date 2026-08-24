//! `drawRect` / `sub_100043C14`, registered at `0x10002D9F8`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawRect",
        lua.create_function(
            move |_,
                  (red, green, blue, alpha, left, top, right, bottom, keep_state): (
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                Value,
            )|
                  -> LuaResult<()> {
                let Value::Boolean(keep_state) = keep_state else {
                    return Err(LuaError::RuntimeError(
                        "drawRect argument 9 must be boolean".to_owned(),
                    ));
                };
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                // sub_100043C14 installs default GL state when the ninth
                // argument is false and deliberately leaves it live.
                if !keep_state {
                    bridge.state = RenderState::default();
                }
                let state = bridge.state;
                bridge.push_rect_command(native_rect_command(
                    [red, green, blue, alpha],
                    left,
                    top,
                    right,
                    bottom,
                    state,
                ));
                Ok(())
            },
        )?,
    )
}
