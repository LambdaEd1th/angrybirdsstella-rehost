//! `drawRect` / `sub_100043C14`, registered at `0x10002D9F8`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawRect",
        lua.create_function(move |_, args: MultiValue| -> LuaResult<()> {
            // sub_1000854B8 reads eight exact NUMBER slots followed by an
            // exact BOOLEAN, and does not reject trailing stack values.
            let red = native_required_number(&args, 0, "drawRect")?;
            let green = native_required_number(&args, 1, "drawRect")?;
            let blue = native_required_number(&args, 2, "drawRect")?;
            let alpha = native_required_number(&args, 3, "drawRect")?;
            let left = native_required_number(&args, 4, "drawRect")?;
            let top = native_required_number(&args, 5, "drawRect")?;
            let right = native_required_number(&args, 6, "drawRect")?;
            let bottom = native_required_number(&args, 7, "drawRect")?;
            let keep_state = native_required_boolean(&args, 8, "drawRect")?;
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
        })?,
    )
}
