//! Skynest storage boundary and local completion behavior for retired cloud APIs.

use super::skynest_account::OfflineState;
use crate::*;
use mlua::{IntoLuaMulti, RegistryKey};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

enum Completion {
    SetKey {
        key: String,
        value: String,
        callback: RegistryKey,
    },
    GetKey {
        key: String,
        callback: RegistryKey,
    },
    GetKeyForAccountIds {
        callback: RegistryKey,
    },
}

/// Retained provider callbacks and deterministic offline storage state.
#[derive(Clone)]
pub(crate) struct SkynestStorageRuntime {
    state: Arc<Mutex<OfflineState>>,
    completions: Rc<RefCell<VecDeque<Completion>>>,
}

impl SkynestStorageRuntime {
    fn new(state: Arc<Mutex<OfflineState>>) -> Self {
        Self {
            state,
            completions: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    fn push(&self, completion: Completion) {
        self.completions.borrow_mut().push_back(completion);
    }

    fn take_pending(&self) -> VecDeque<Completion> {
        std::mem::take(&mut *self.completions.borrow_mut())
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    state: Arc<Mutex<OfflineState>>,
) -> LuaResult<SkynestStorageRuntime> {
    let runtime = SkynestStorageRuntime::new(state);
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

    let set_runtime = runtime.clone();
    storage.set(
        "native_setKey",
        lua.create_function(move |lua, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_setKey")?;
            let value = native_required_string(&args, 1, "SkynestStorage.native_setKey")?;
            let callback = native_required_function(&args, 2, "SkynestStorage.native_setKey")?;
            set_runtime.push(Completion::SetKey {
                key,
                value,
                callback: lua.create_registry_value(callback)?,
            });
            Ok(())
        })?,
    )?;

    let get_runtime = runtime.clone();
    storage.set(
        "native_getKey",
        lua.create_function(move |lua, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_getKey")?;
            let callback = native_required_function(&args, 1, "SkynestStorage.native_getKey")?;
            get_runtime.push(Completion::GetKey {
                key,
                callback: lua.create_registry_value(callback)?,
            });
            Ok(())
        })?,
    )?;

    let batch_runtime = runtime.clone();
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
            batch_runtime.push(Completion::GetKeyForAccountIds {
                callback: lua.create_registry_value(callback)?,
            });
            Ok(())
        })?,
    )?;

    globals.set("SkynestStorage", storage)?;
    Ok(runtime)
}

/// Deliver retained storage-provider callbacks at the application frame head.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &SkynestStorageRuntime) -> LuaResult<()> {
    // Provider callbacks may submit another request. Purple cannot complete
    // that nested request on the same retained-provider callback stack.
    for completion in runtime.take_pending() {
        match completion {
            Completion::SetKey {
                key,
                value,
                callback,
            } => {
                runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                    .keys
                    .insert(key, value);
                call_retained(lua, callback, ())?;
            }
            Completion::GetKey { key, callback } => {
                let value = runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                    .keys
                    .get(&key)
                    .cloned();
                match value {
                    // sub_1000BBC14 supplies the retrieved string.
                    Some(value) => call_retained(lua, callback, value)?,
                    // sub_1000BBAA8 is the zero-argument failure completion.
                    None => call_retained(lua, callback, ())?,
                }
            }
            Completion::GetKeyForAccountIds { callback } => {
                // The offline account has no shared account id, so the
                // provider-success map is empty.
                let values = lua.create_table()?;
                call_retained(lua, callback, values)?;
            }
        }
    }
    Ok(())
}

fn call_retained(lua: &Lua, callback: RegistryKey, args: impl IntoLuaMulti) -> LuaResult<()> {
    let function = lua.registry_value::<mlua::Function>(&callback)?;
    let result = function.call::<()>(args);
    lua.remove_registry_value(callback)?;
    result
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
