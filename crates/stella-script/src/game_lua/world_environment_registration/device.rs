//! Notification, sensor and platform-query members.

use crate::*;

mod theme_refresh;

pub(super) fn install_notification(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setNotificationCallback",
        lua.create_function(move |_, args: MultiValue| {
            let callback = native_required_string(&args, 0, "setNotificationCallback")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .notification_callback = Some(callback);
            Ok(())
        })?,
    )
}

pub(super) fn install_theme_refresh(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    theme_refresh::install(lua, globals, render)
}

pub(super) fn install_accelerometer(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setAccelerometerActive",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "setAccelerometerActive")?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // `sub_10004C524` starts/stops the supported platform sensor,
            // records the effective active flag and then clears the adjacent
            // filtered pair with one 64-bit store on every call.
            bridge.accelerometer_active = enabled;
            bridge.accelerometer_filtered = [0.0; 2];
            Ok(())
        })?,
    )
}

pub(super) fn install_orientation(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_getDeviceOrientation",
        lua.create_function(move |_, _: MultiValue| {
            let orientation = render
                .lock()
                .expect("render bridge lock poisoned")
                .device_orientation_index;
            // dword_1009AF090, selected by sub_100051298 after calling the
            // gr::Context virtual member at +0x120.
            Ok([0_i32, 90, 180, 270]
                .get(orientation as usize)
                .copied()
                .unwrap_or(-1))
        })?,
    )
}

pub(super) fn install_os_name(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set("native_getOSName", lua.create_function(|_, ()| Ok("iOS"))?)
}
