//! Late one-string Lua file failure callback registration.

use crate::*;
use std::{cell::Cell, rc::Rc};

pub(crate) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let native_identity = Rc::new(Cell::new(std::ptr::null()));
    let callback_identity = Rc::clone(&native_identity);
    let relay = lua.create_function(move |lua, args: MultiValue| {
        // sub_100089E6C strictly consumes one string before dispatching
        // the callback relay at sub_100056950.
        let message = native_required_string(&args, 0, "onLoadLuaFileFail")?;
        // LuaObject::call uses the owner's normal field lookup, including
        // its metatable. Before scripts replace the native fallback, make
        // the recursive/uninitialized relay an explicit host error.
        let callback = game_environment(lua)?.get::<mlua::Function>("onLoadLuaFileFail")?;
        if callback.to_pointer() == callback_identity.get() {
            return Err(runtime_error(
                "onLoadLuaFileFail script callback is not installed",
            ));
        }
        callback.call::<()>(message)
    })?;
    native_identity.set(relay.to_pointer());
    globals.set("onLoadLuaFileFail", relay)?;
    Ok(())
}
