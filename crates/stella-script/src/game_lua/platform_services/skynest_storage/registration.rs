use super::*;

pub(in super::super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    state: Arc<Mutex<OfflineState>>,
    account: SkynestAccountRuntime,
    application_events: ApplicationEventScheduler,
) -> LuaResult<SkynestStorageRuntime> {
    let runtime = SkynestStorageRuntime::new(state, account, application_events);
    let storage = lua.create_table()?;
    let load_runtime = runtime.clone();
    storage.set(
        "native_loadCloudSettings",
        lua.create_function(move |_, _: MultiValue| {
            let (online, owner) = load_runtime.online_config()?;
            // sub_1000BAE48 returns false while another cloud transaction is live.
            if !load_runtime.start_transaction(&owner, online.is_some())? {
                return Ok(false);
            }
            if let Some(config) = online {
                if let Err(error) = load_runtime.spawn_online(owner.clone(), move || {
                    OnlineCompletion::LoadCloudSettings(request_get(&config, CLOUD_SETTINGS_KEY))
                }) {
                    load_runtime.finish_transaction(&owner)?;
                    return Err(error);
                }
            } else {
                load_runtime.push(owner, Completion::LoadCloudSettings);
            }
            Ok(true)
        })?,
    )?;
    let save_runtime = runtime.clone();
    storage.set(
        "native_saveCloudSettings",
        lua.create_function(move |lua, args: MultiValue| {
            let table = native_required_table(&args, 0, "SkynestStorage.native_saveCloudSettings")?;
            let (online, owner) = save_runtime.online_config()?;
            if let Some(config) = online {
                // The recovered wire value is Lua assignments, not JSON.
                // Serialize the original table before creating a transaction.
                let encoded = cloud_payload::encode(&table)?;
                if !save_runtime.start_transaction(&owner, true)? {
                    return Ok(false);
                }
                let hash = save_runtime
                    .online_cache
                    .borrow()
                    .hashes
                    .get(CLOUD_SETTINGS_KEY)
                    .cloned()
                    .unwrap_or_default();
                if let Err(error) = save_runtime.spawn_online(owner.clone(), move || {
                    let result = request_set(&config, CLOUD_SETTINGS_KEY, &encoded, &hash);
                    OnlineCompletion::SaveCloudSettings { config, result }
                }) {
                    save_runtime.finish_transaction(&owner)?;
                    return Err(error);
                }
            } else {
                // The explicit local provider keeps its existing JSON document.
                let document = lua.from_value::<serde_json::Value>(Value::Table(table))?;
                if !save_runtime.start_transaction(&owner, false)? {
                    return Ok(false);
                }
                save_runtime.push(owner, Completion::SaveCloudSettings(document));
            }
            Ok(true)
        })?,
    )?;
    let timeout_runtime = runtime.clone();
    storage.set(
        "native_setRequestTimeout",
        lua.create_function(move |_, args: MultiValue| {
            let seconds =
                native_required_number(&args, 0, "SkynestStorage.native_setRequestTimeout")?;
            let timeout =
                Duration::try_from_secs_f64(seconds.max(0.0)).unwrap_or(DEFAULT_REQUEST_TIMEOUT);
            *timeout_runtime
                .request_timeout
                .lock()
                .map_err(|_| runtime_error("storage request-timeout lock poisoned"))? = timeout;
            Ok(())
        })?,
    )?;
    let transaction_runtime = runtime.clone();
    storage.set(
        "native_isTransactionInProcess",
        lua.create_function(move |_, _: MultiValue| {
            transaction_runtime.synchronize_owner()?;
            Ok(transaction_runtime
                .state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .transaction_in_progress)
        })?,
    )?;

    let set_runtime = runtime.clone();
    storage.set(
        "native_setKey",
        lua.create_function(move |lua, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_setKey")?;
            let value = native_required_string(&args, 1, "SkynestStorage.native_setKey")?;
            let callback = native_required_function(&args, 2, "SkynestStorage.native_setKey")?;
            let (online, owner) = set_runtime.online_config()?;
            if let Some(config) = online {
                let request_id = set_runtime.retain_callback(lua, callback)?;
                let hash = set_runtime
                    .online_cache
                    .borrow()
                    .hashes
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                let queued_key = key.clone();
                let queued_value = value.clone();
                if let Err(error) = set_runtime.spawn_online(owner, move || {
                    let result = request_set(&config, &key, &value, &hash);
                    OnlineCompletion::SetKey {
                        request_id,
                        key: queued_key,
                        value: queued_value,
                        config,
                        result,
                    }
                }) {
                    set_runtime.remove_callback(lua, request_id)?;
                    return Err(error);
                }
            } else {
                set_runtime.push(
                    owner,
                    Completion::SetKey {
                        key,
                        value,
                        callback: lua.create_registry_value(callback)?,
                    },
                );
            }
            Ok(())
        })?,
    )?;

    let get_runtime = runtime.clone();
    storage.set(
        "native_getKey",
        lua.create_function(move |lua, args: MultiValue| {
            let key = native_required_string(&args, 0, "SkynestStorage.native_getKey")?;
            let callback = native_required_function(&args, 1, "SkynestStorage.native_getKey")?;
            let (online, owner) = get_runtime.online_config()?;
            if let Some(config) = online {
                let request_id = get_runtime.retain_callback(lua, callback)?;
                let queued_key = key.clone();
                if let Err(error) =
                    get_runtime.spawn_online(owner, move || OnlineCompletion::GetKey {
                        request_id,
                        key: queued_key,
                        result: request_get(&config, &key),
                    })
                {
                    get_runtime.remove_callback(lua, request_id)?;
                    return Err(error);
                }
            } else {
                get_runtime.push(
                    owner,
                    Completion::GetKey {
                        key,
                        callback: lua.create_registry_value(callback)?,
                    },
                );
            }
            Ok(())
        })?,
    )?;

    let batch_runtime = runtime.clone();
    storage.set(
        "native_getKeyForAccountIds",
        lua.create_function(move |lua, args: MultiValue| {
            let key =
                native_required_string(&args, 0, "SkynestStorage.native_getKeyForAccountIds")?;
            let account_ids =
                native_required_table(&args, 1, "SkynestStorage.native_getKeyForAccountIds")?;
            let callback =
                native_required_function(&args, 2, "SkynestStorage.native_getKeyForAccountIds")?;

            // sub_1000BA85C consumes the contiguous string sequence and stops
            // at the first non-string value. The offline account has no
            // shared account id, so the successful result map is empty.
            let mut contiguous_ids = Vec::new();
            for value in account_ids.sequence_values::<Value>() {
                match value? {
                    Value::String(value) => contiguous_ids.push(value.to_str()?.to_owned()),
                    _ => break,
                }
            }
            let (online, owner) = batch_runtime.online_config()?;
            if let Some(config) = online {
                let request_id = batch_runtime.retain_callback(lua, callback)?;
                if let Err(error) = batch_runtime.spawn_online(owner, move || {
                    OnlineCompletion::GetKeyForAccountIds {
                        request_id,
                        result: request_batch(&config, &key, &contiguous_ids),
                    }
                }) {
                    batch_runtime.remove_callback(lua, request_id)?;
                    return Err(error);
                }
            } else {
                batch_runtime.push(
                    owner,
                    Completion::GetKeyForAccountIds {
                        key,
                        account_ids: contiguous_ids,
                        callback: lua.create_registry_value(callback)?,
                    },
                );
            }
            Ok(())
        })?,
    )?;

    globals.set("SkynestStorage", storage)?;
    Ok(runtime)
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
