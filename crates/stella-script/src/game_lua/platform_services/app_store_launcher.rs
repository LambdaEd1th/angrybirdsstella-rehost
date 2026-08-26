//! Cross-promotion metadata and native application/store launch boundary.

use crate::*;

#[derive(Debug, Default)]
struct AppStoreLauncherState {
    launch_id: String,
    store_id: String,
    installed: bool,
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let launcher = lua.create_table()?;
    let state = Arc::new(Mutex::new(AppStoreLauncherState::default()));

    let update_root = Arc::clone(&data_root);
    let update_state = Arc::clone(&state);
    launcher.set(
        "updateGameData",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_10009EAC4 reads slot one with the exact
            // STRING-tag accessor and ignores the tail. sub_10009E44C then
            // calls GameLua's text pipeline with encrypted=true,
            // alternateKey=true and decompress=false before parsing JSON.
            let requested = native_required_string(&args, 0, "AppStoreLauncher.updateGameData")?;
            let bytes =
                game_lua::text_files::load_text_bytes(&update_root, &requested, true, true, false)
                    .map_err(runtime_error)?;
            let document: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|error| runtime_error(error.to_string()))?;
            let launch_id = document
                .get("launchId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| runtime_error("cross-promotion launchId must be a string"))?;
            let store_id = document
                .get("storeId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| runtime_error("cross-promotion storeId must be a string"))?;

            let mut state = update_state
                .lock()
                .expect("app-store launcher state lock poisoned");
            state.launch_id.clear();
            state.launch_id.push_str(launch_id);
            state.store_id.clear();
            state.store_id.push_str(store_id);
            // Purple asks UIApplication whether launchId can be opened and
            // retains that boolean at +0x10. A cross-platform offline host
            // has no installed mobile application registered for the scheme.
            state.installed = false;
            Ok(())
        })?,
    )?;

    launcher.set(
        "launchAppStore",
        lua.create_function(move |_, _: MultiValue| {
            let state = state
                .lock()
                .expect("app-store launcher state lock poisoned");
            if state.installed {
                // sub_10009E940 opens launchId directly only when the cached
                // canOpenURL result is true. Retain that request in the same
                // host URL bridge if a future platform backend reports it.
                render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .requested_url = Some(state.launch_id.clone());
                return Ok(());
            }

            // The non-installed branch passes storeId and literal type 3 to
            // the native StoreKit launcher (sub_1005340D0). Retain the
            // request in the same host bridge used by ForceUpdate rather
            // than causing a nondeterministic external side effect.
            render
                .lock()
                .expect("render bridge lock poisoned")
                .requested_app_store_product = Some((state.store_id.clone(), 3));
            Ok(())
        })?,
    )?;

    globals.set("AppStoreLauncher", launcher)?;
    Ok(())
}
