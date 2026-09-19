use super::*;

/// Deliver retained storage-provider callbacks at the application frame head.
pub(crate) fn dispatch_local_completion(
    lua: &Lua,
    runtime: &SkynestStorageRuntime,
) -> LuaResult<()> {
    runtime.synchronize_owner()?;
    let Some(Queued { owner, completion }) = runtime.pop_pending() else {
        return Ok(());
    };
    if !owner.is_current() {
        return lifecycle::discard_local_callback(lua, completion);
    }
    runtime.finish_transaction(&owner)?;
    match completion {
        Completion::LoadCloudSettings => {
            let cloud_settings = {
                let state = runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
                state.cloud_settings.clone()
            };
            match cloud_settings {
                Some(document) => {
                    let Value::Table(table) = lua.to_value(&document)? else {
                        return Err(runtime_error("cloud settings root is not a table"));
                    };
                    call_native_member(lua, "cloudDataSync", table)?;
                }
                None => {
                    notify_event(lua, "EID_SYNC_CLOUD_LOAD_FAILED")?;
                    call_native_member(lua, "cloudDataFirstSync", ())?;
                }
            }
        }
        Completion::SaveCloudSettings(document) => {
            {
                let mut state = runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
                state.cloud_settings = Some(document);
                state.persist()?;
            }
            notify_event(lua, "EID_SYNC_CLOUD_COMPLETED")?;
        }
        Completion::SetKey {
            key,
            value,
            callback,
        } => {
            {
                let mut state = runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
                state.keys.insert(key, value);
                state.persist()?;
            }
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
        Completion::GetKeyForAccountIds {
            key,
            account_ids,
            callback,
        } => {
            let values = lua.create_table()?;
            let state = runtime
                .state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
            if state.local_provider
                && state.logged_in
                && account_ids.iter().any(|account| account == "local-player")
                && let Some(value) = state.keys.get(&key)
            {
                values.set("local-player", value.clone())?;
            }
            drop(state);
            call_retained(lua, callback, values)?;
        }
    }
    Ok(())
}

pub(crate) fn dispatch_online_completion(
    lua: &Lua,
    runtime: &SkynestStorageRuntime,
) -> LuaResult<()> {
    runtime.synchronize_owner()?;
    let Some(Queued { owner, completion }) = runtime.pop_online_pending() else {
        return Ok(());
    };
    if !owner.is_current() {
        if let Some(request_id) = completion.request_id() {
            runtime.remove_callback(lua, request_id)?;
        }
        return Ok(());
    }
    match completion {
        OnlineCompletion::LoadCloudSettings(result) => {
            runtime.finish_transaction(&owner)?;
            let value = match result {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("storage cloud settings load failed: {}", error.message);
                    notify_event(lua, "EID_SYNC_CLOUD_LOAD_FAILED")?;
                    if error.status == Some(404) {
                        call_native_member(lua, "cloudDataFirstSync", ())?;
                    }
                    return Ok(());
                }
            };
            let table = match cloud_payload::decode(lua, &value.value) {
                Ok(table) => table,
                Err(_) => {
                    // Remote Lua text executes only in the bounded data parser.
                    // Bad content cannot terminate the game or publish a hash.
                    eprintln!("storage cloud settings payload rejected");
                    notify_event(lua, "EID_SYNC_CLOUD_LOAD_FAILED")?;
                    return Ok(());
                }
            };
            runtime
                .online_cache
                .borrow_mut()
                .hashes
                .insert(CLOUD_SETTINGS_KEY.to_owned(), value.hash);
            call_native_member(lua, "cloudDataSync", table)?;
        }
        OnlineCompletion::SaveCloudSettings { config, result } => {
            if result
                .as_ref()
                .is_err_and(|error| error.status == Some(409))
            {
                // 10070093C -> AppScheduler -> 10070CD74 starts the conflict
                // GET. Do not finish busy or invoke Lua in this first snapshot.
                if let Err(error) = runtime.spawn_online(owner.clone(), move || {
                    OnlineCompletion::SaveCloudSettingsConflict(request_get(
                        &config,
                        CLOUD_SETTINGS_KEY,
                    ))
                }) {
                    runtime.finish_transaction(&owner)?;
                    return Err(error);
                }
                return Ok(());
            }
            runtime.finish_transaction(&owner)?;
            match result {
                Ok(result) => {
                    runtime
                        .online_cache
                        .borrow_mut()
                        .hashes
                        .insert(CLOUD_SETTINGS_KEY.to_owned(), result.hash);
                    notify_event(lua, "EID_SYNC_CLOUD_COMPLETED")?;
                }
                Err(error) => {
                    deliver_cloud_save_error(lua, runtime, &owner, error_code(&error), None)?
                }
            }
        }
        OnlineCompletion::SaveCloudSettingsConflict(result) => {
            runtime.finish_transaction(&owner)?;
            match result {
                Ok(remote) => deliver_cloud_save_error(lua, runtime, &owner, 3, Some(remote))?,
                Err(error) => {
                    deliver_cloud_save_error(lua, runtime, &owner, error_code(&error), None)?
                }
            }
        }
        OnlineCompletion::SetKey {
            request_id,
            key,
            value,
            config,
            result,
        } => {
            if result
                .as_ref()
                .is_err_and(|error| error.status == Some(409))
            {
                let conflict_key = key.clone();
                if let Err(error) =
                    runtime.spawn_online(owner, move || OnlineCompletion::SetKeyConflict {
                        request_id,
                        key,
                        result: request_get(&config, &conflict_key),
                    })
                {
                    runtime.remove_callback(lua, request_id)?;
                    return Err(error);
                }
                return Ok(());
            }
            if let Ok(result) = result {
                let mut cache = runtime.online_cache.borrow_mut();
                cache.keys.insert(key.clone(), value);
                cache.hashes.insert(key, result.hash);
            }
            if let Some(callback) = runtime.take_callback(request_id) {
                call_retained(lua, callback, ())?;
            }
        }
        OnlineCompletion::SetKeyConflict {
            request_id,
            key,
            result,
        } => {
            if let Ok(remote) = result {
                // Native updates the conflict GET's hash, not the submitted
                // local value or a separate decoded-value cache.
                runtime
                    .online_cache
                    .borrow_mut()
                    .hashes
                    .insert(key, remote.hash);
            }
            if let Some(callback) = runtime.take_callback(request_id) {
                // 1000BBD90 ignores key/error/local/remote and calls with no args.
                call_retained(lua, callback, ())?;
            }
        }
        OnlineCompletion::GetKey {
            request_id,
            key,
            result,
        } => {
            let callback_value = match result {
                Ok(value) => {
                    let mut cache = runtime.online_cache.borrow_mut();
                    cache.keys.insert(key.clone(), value.value.clone());
                    cache.hashes.insert(key, value.hash);
                    Some(value.value)
                }
                Err(error) => {
                    eprintln!("storage key read failed: {}", error.message);
                    None
                }
            };
            if let Some(callback) = runtime.take_callback(request_id) {
                match callback_value {
                    Some(value) => call_retained(lua, callback, value)?,
                    None => call_retained(lua, callback, ())?,
                }
            }
        }
        OnlineCompletion::GetKeyForAccountIds { request_id, result } => {
            if let Some(callback) = runtime.take_callback(request_id) {
                match result {
                    Ok(result) => {
                        let values = lua.create_table()?;
                        for (account_id, value) in result {
                            values.set(account_id, value)?;
                        }
                        call_retained(lua, callback, values)?;
                    }
                    Err(error) => {
                        eprintln!("storage account-key read failed: {}", error.message);
                        call_retained(lua, callback, ())?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// 1007097E4: HTTP transport (-1) is provider code 5; authentication statuses
/// are ordinary code 4, not a special reason to suppress cloud completion.
fn error_code(error: &ServiceError) -> u8 {
    match error.status {
        None => 5,
        Some(400) => 1,
        Some(409) => 3,
        Some(404) => 2,
        Some(_) => 4,
    }
}

fn deliver_cloud_save_error(
    lua: &Lua,
    runtime: &SkynestStorageRuntime,
    owner: &RequestOwner,
    code: u8,
    remote: Option<StoredValue>,
) -> LuaResult<()> {
    // 1000BB598 clears busy before this handler. Only provider code 5 skips
    // completed. Code 3 parses remote assignments, invokes merge, then emits.
    if code == 5 {
        return Ok(());
    }
    if code == 3 {
        let text = remote.as_ref().map_or("", |value| value.value.as_str());
        let table = match cloud_payload::decode(lua, text) {
            Ok(table) => table,
            Err(_) => {
                // Explicit host safeguard: native hash publication precedes
                // Lua parsing, whereas rejected remote data is not published
                // here. Preserve native no-merge/no-completed error ordering.
                eprintln!("storage conflict payload rejected");
                return Ok(());
            }
        };
        if let Some(remote) = remote {
            runtime
                .online_cache
                .borrow_mut()
                .hashes
                .insert(CLOUD_SETTINGS_KEY.to_owned(), remote.hash);
        }
        call_native_member(lua, "cloudDataNewDataAvailable", table)?;
        // Host lifetime boundary: merge may reenter logout/provider switching.
        // A new save for this same identity is allowed and still receives the
        // old completed event, so do not compare transaction sequence numbers.
        if !owner.is_current() {
            return Ok(());
        }
    }
    notify_event(lua, "EID_SYNC_CLOUD_COMPLETED")
}

fn call_native_member(lua: &Lua, name: &str, args: impl IntoLuaMulti) -> LuaResult<()> {
    let Value::Table(storage) = lua.globals().get::<Value>("SkynestStorage")? else {
        return Ok(());
    };
    let Value::Function(function) = storage.get::<Value>(name)? else {
        return Ok(());
    };
    function.call::<()>(args)
}

fn notify_event(lua: &Lua, name: &str) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Function(notify) = environment.get::<Value>("notifyEventManager")? else {
        return Ok(());
    };
    notify.call::<()>((name, lua.create_table()?))
}

fn call_retained(lua: &Lua, callback: RegistryKey, args: impl IntoLuaMulti) -> LuaResult<()> {
    let function = lua.registry_value::<mlua::Function>(&callback)?;
    let result = function.call::<()>(args);
    lua.remove_registry_value(callback)?;
    result
}
