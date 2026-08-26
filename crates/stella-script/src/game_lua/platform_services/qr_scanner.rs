//! Native QR/Telepods camera boundary for hosts without a scanner backend.

use crate::*;
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let scanner = lua.create_table()?;

    scanner.set(
        "isCameraSupported",
        lua.create_function(|_, _: MultiValue| {
            // QrScanner::isCameraSupported (sub_1000DE050) first asks the
            // platform camera service whether any device exists, then accepts
            // either camera side. The desktop host has no scanner capture
            // backend, so this is the native no-device branch.
            Ok(false)
        })?,
    )?;
    scanner.set(
        "isFrontCameraSupported",
        lua.create_function(|_, _: MultiValue| {
            // sub_1000DE094 returns false before querying camera side 2 when
            // the platform reports no capture devices.
            Ok(false)
        })?,
    )?;

    for method in ["start", "stop"] {
        scanner.set(
            method,
            lua.create_function(|_, _: MultiValue| {
                // sub_1000DE0C0 cannot allocate a QrScannerSession without a
                // supported camera; sub_1000DE1A8 then has no session to
                // release. Both generated adapters return zero Lua results.
                Ok(())
            })?,
        )?;
    }

    // The native object retains one LuaFunction at +0x88. Rc/RefCell follows
    // mlua's single-state ownership and lets the registered closure keep the
    // same strong reference until a later function or non-function replaces
    // it. There is no scanner event source on this host to invoke it.
    let recognized_callback = Rc::new(RefCell::new(None::<mlua::Function>));
    scanner.set(
        "setQrRecognizedCallback",
        lua.create_function(move |_, args: MultiValue| {
            // Direct member sub_1000DE1F4 tests slot one with isFunction. A
            // function is retained; nil and every other Lua tag clear the
            // previous callback without raising a type error. Extra values
            // are never inspected and the member returns zero results.
            *recognized_callback.borrow_mut() = match args.front() {
                Some(Value::Function(callback)) => Some(callback.clone()),
                _ => None,
            };
            Ok(())
        })?,
    )?;

    globals.set("QrScanner", scanner)?;
    Ok(())
}
