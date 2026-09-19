//! GameLua+0x570's load diagnostics and GameApp's post-gamelogic delivery.

use mlua::{Function, Lua, Result as LuaResult};

use crate::game_environment;

#[derive(Default)]
struct PersistentLoadMessages(Vec<String>);

pub(super) fn queue_persistent_load_message(lua: &Lua, message: String) {
    if lua.app_data_ref::<PersistentLoadMessages>().is_none() {
        lua.set_app_data(PersistentLoadMessages::default());
    }
    lua.app_data_mut::<PersistentLoadMessages>()
        .expect("persistent load message store was initialized")
        .0
        .push(message);
}

pub(crate) fn pending_persistent_load_messages(lua: &Lua) -> Vec<String> {
    lua.app_data_ref::<PersistentLoadMessages>()
        .map(|messages| messages.0.clone())
        .unwrap_or_default()
}

pub(crate) fn deliver_persistent_load_messages(lua: &Lua) -> LuaResult<()> {
    // 100027268..2CC reloads the vector bounds and callback on every iteration.
    // A callback can append another message. Clear only after the entire loop;
    // a throwing callback leaves even previously delivered messages retained.
    let mut index = 0;
    loop {
        let message = lua
            .app_data_ref::<PersistentLoadMessages>()
            .and_then(|messages| messages.0.get(index).cloned());
        let Some(message) = message else { break };
        game_environment(lua)?
            .get::<Function>("onLoadLuaFileFail")?
            .call::<()>(message)?;
        index += 1;
    }
    if let Some(mut messages) = lua.app_data_mut::<PersistentLoadMessages>() {
        messages.0.clear();
    }
    Ok(())
}
