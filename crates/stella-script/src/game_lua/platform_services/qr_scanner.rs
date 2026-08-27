//! Native QR/Telepods scanner boundary and host-code injection bridge.

use crate::*;

const REGISTRY_STATE: &str = "stella.qr_scanner.state";

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let scanner = lua.create_table()?;
    let state = lua.create_table()?;
    state.set("available", false)?;
    state.set("front_available", false)?;
    state.set("active", false)?;
    state.set("callback", Value::Nil)?;
    state.set("pending_code", Value::Nil)?;
    lua.set_named_registry_value(REGISTRY_STATE, state.clone())?;

    let camera_state = state.clone();
    scanner.set(
        "isCameraSupported",
        lua.create_function(move |_, _: MultiValue| {
            // QrScanner::isCameraSupported (sub_1000DE050) first asks the
            // platform camera service whether any device exists, then accepts
            // either camera side. Host injection reports one virtual scanner;
            // the ordinary desktop path leaves it unavailable.
            camera_state.get::<bool>("available")
        })?,
    )?;
    let front_state = state.clone();
    scanner.set(
        "isFrontCameraSupported",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1000DE094 returns false before querying camera side 2 when
            // the platform reports no capture devices. The virtual host
            // scanner never claims a front-facing camera.
            front_state.get::<bool>("front_available")
        })?,
    )?;

    let start_state = state.clone();
    scanner.set(
        "start",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1000DE0C0 allocates the platform session and begins its
            // capture source. A queued host code is the virtual session's
            // first recognized frame.
            start_state.set("active", true)?;
            dispatch_pending(&start_state).map(|_| ())
        })?,
    )?;
    let stop_state = state.clone();
    scanner.set(
        "stop",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1000DE1A8 releases the active scanner session while keeping
            // the retained recognized callback independently owned.
            stop_state.set("active", false)
        })?,
    )?;

    let callback_state = state;
    scanner.set(
        "setQrRecognizedCallback",
        lua.create_function(move |_, args: MultiValue| {
            // Direct member sub_1000DE1F4 tests slot one with isFunction. A
            // function is retained; nil and every other Lua tag clear the
            // previous callback without raising a type error. Extra values
            // are never inspected and the member returns zero results.
            match args.front() {
                Some(Value::Function(callback)) => {
                    callback_state.set("callback", callback.clone())?;
                    dispatch_pending(&callback_state).map(|_| ())
                }
                _ => callback_state.set("callback", Value::Nil),
            }
        })?,
    )?;

    globals.set("QrScanner", scanner)?;
    Ok(())
}

pub(crate) fn set_host_available(lua: &Lua, available: bool) -> LuaResult<()> {
    let state: mlua::Table = lua.named_registry_value(REGISTRY_STATE)?;
    state.set("available", available)
}

pub(crate) fn submit_host_code(lua: &Lua, code: &str) -> LuaResult<bool> {
    let state: mlua::Table = lua.named_registry_value(REGISTRY_STATE)?;
    state.set("pending_code", code)?;
    dispatch_pending(&state)
}

fn dispatch_pending(state: &mlua::Table) -> LuaResult<bool> {
    if !state.get::<bool>("available")? || !state.get::<bool>("active")? {
        return Ok(false);
    }
    let Value::Function(callback) = state.get::<Value>("callback")? else {
        return Ok(false);
    };
    let Value::String(code) = state.get::<Value>("pending_code")? else {
        return Ok(false);
    };
    // Clear first: TelepodPage stops the scanner and may synchronously enter
    // the IAP wallet callbacks while the recognized callback is running.
    state.set("pending_code", Value::Nil)?;
    callback.call::<()>(code)?;
    Ok(true)
}
