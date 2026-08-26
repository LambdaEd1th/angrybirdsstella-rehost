//! Skynest storage boundary and local completion behavior for retired cloud APIs.

use super::skynest_account::OfflineState;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    state: Arc<Mutex<OfflineState>>,
) -> LuaResult<()> {
    let storage = lua.create_table()?;
    storage.set(
        "native_loadCloudSettings",
        // sub_1000BAE48 returns false when no transaction can be started.
        // A signed-out offline host has no remote account state to load.
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    storage.set(
        "native_saveCloudSettings",
        lua.create_function(|_, args: MultiValue| {
            native_required_table(&args, 0, "SkynestStorage.native_saveCloudSettings")?;
            Ok(false)
        })?,
    )?;
    storage.set(
        "native_setRequestTimeout",
        lua.create_function(|_, args: MultiValue| {
            native_required_number(&args, 0, "SkynestStorage.native_setRequestTimeout")?;
            Ok(())
        })?,
    )?;
    storage.set(
        "native_isTransactionInProcess",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;

    let set_state = Arc::clone(&state);
    storage.set(
        "native_setKey",
        lua.create_function(move |_, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_setKey")?;
            let value = native_required_string(&args, 1, "SkynestStorage.native_setKey")?;
            let callback = native_required_function(&args, 2, "SkynestStorage.native_setKey")?;
            set_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .keys
                .insert(key, value);
            // Both native success and error completions call the LuaFunction
            // with no arguments; completing locally keeps the menu coroutine
            // from waiting on the retired server.
            callback.call::<()>(())
        })?,
    )?;

    let get_state = Arc::clone(&state);
    storage.set(
        "native_getKey",
        lua.create_function(move |_, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_getKey")?;
            let callback = native_required_function(&args, 1, "SkynestStorage.native_getKey")?;
            let value = get_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .keys
                .get(&key)
                .cloned();
            match value {
                // sub_1000BBC14 supplies the retrieved string.
                Some(value) => callback.call::<()>(value),
                // sub_1000BBAA8 is the zero-argument failure completion.
                None => callback.call::<()>(()),
            }
        })?,
    )?;

    storage.set(
        "native_getKeyForAccountIds",
        lua.create_function(move |lua, args: MultiValue| {
            native_required_string(&args, 0, "SkynestStorage.native_getKeyForAccountIds")?;
            let account_ids =
                native_required_table(&args, 1, "SkynestStorage.native_getKeyForAccountIds")?;
            let callback =
                native_required_function(&args, 2, "SkynestStorage.native_getKeyForAccountIds")?;

            // sub_1000BA85C consumes the contiguous string sequence and stops
            // at the first non-string value. The offline account has no
            // shared account id, so the successful result map is empty.
            for value in account_ids.sequence_values::<Value>() {
                if !matches!(value?, Value::String(_)) {
                    break;
                }
            }
            let values = lua.create_table()?;
            callback.call::<()>(values)
        })?,
    )?;

    globals.set("SkynestStorage", storage)?;
    Ok(())
}

fn native_required_function(
    args: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<mlua::Function> {
    match args.iter().nth(index) {
        Some(Value::Function(callback)) => Ok(callback.clone()),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (function expected)",
            index + 1
        ))),
    }
}
