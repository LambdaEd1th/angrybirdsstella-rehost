//! World/camera control scalars owned by GameLua.

use crate::*;

pub(super) fn install_smooth_zoom(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "enableSmoothZooming",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "enableSmoothZooming")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .smooth_zooming = enabled;
            Ok(())
        })?,
    )
}

pub(super) fn install_gravity(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setWorldGravity",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100088294 requires NUMBER in slots one and two.
            let gravity_x = f64::from(native_required_number(&args, 0, "setWorldGravity")? as f32);
            let gravity_y = f64::from(native_required_number(&args, 1, "setWorldGravity")? as f32);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.world_gravity_x = gravity_x;
            bridge.world_gravity_y = gravity_y;
            Ok(())
        })?,
    )
}

pub(super) fn install_editing(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setEditing",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "setEditing")?;
            render.lock().expect("render bridge lock poisoned").editing = enabled;
            Ok(())
        })?,
    )
}

pub(super) fn install_mouse_wheel(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "resetMouseWheelScale",
        lua.create_function(move |_, args: MultiValue| {
            let scale = f64::from(native_required_number(&args, 0, "resetMouseWheelScale")? as f32);
            let scale = scale as f32;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // sub_100043980 writes both GameApp+0x4FC and +0x51C.
            bridge.input_zoom.current = scale;
            bridge.input_zoom.previous = scale;
            Ok(())
        })?,
    )
}
