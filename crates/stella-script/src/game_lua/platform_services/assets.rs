//! Downloadable Assets service and named sprite-sheet lifetime.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let downloadable_assets = lua.create_table()?;
    let downloaded_asset_names = Arc::new(Mutex::new(BTreeMap::<String, String>::new()));
    let downloaded_query_root = Arc::clone(&data_root);
    let downloaded_query_names = Arc::clone(&downloaded_asset_names);
    downloadable_assets.set(
        "haveBeenDownloaded",
        lua.create_function(move |_, args: MultiValue| {
            if args.is_empty() {
                return Ok(false);
            }
            let names = downloaded_query_names
                .lock()
                .expect("downloaded asset map lock poisoned");
            Ok(args.iter().all(|value| {
                value_string(value).is_some_and(|requested| {
                    names.contains_key(&requested)
                        || app_data_path(&downloaded_query_root, &requested)
                            .is_ok_and(|path| path.is_file())
                })
            }))
        })?,
    )?;
    let downloaded_filename_root = Arc::clone(&data_root);
    let downloaded_filename_names = Arc::clone(&downloaded_asset_names);
    downloadable_assets.set(
        "getAssetFilename",
        lua.create_function(move |_, requested: String| {
            if let Some(filename) = downloaded_filename_names
                .lock()
                .expect("downloaded asset map lock poisoned")
                .get(&requested)
                .cloned()
            {
                return Ok(Some(filename));
            }
            Ok(app_data_path(&downloaded_filename_root, &requested)
                .is_ok_and(|path| path.is_file())
                .then_some(requested))
        })?,
    )?;
    let load_asset_root = Arc::clone(&data_root);
    let load_asset_names = Arc::clone(&downloaded_asset_names);
    downloadable_assets.set(
        "loadFiles",
        lua.create_function(move |lua, requested: mlua::Table| {
            // Assets::loadFiles (sub_1000AC25C) traverses every table value,
            // starts the asynchronous RCS request and later calls exactly one
            // of onLoadSuccess(table) or onLoadError(array, code, message).
            // The discontinued service is represented by its on-disk cache.
            let mut requested_names = Vec::new();
            for pair in requested.pairs::<Value, Value>() {
                let (_, value) = pair?;
                let name = value_string(&value).ok_or_else(|| {
                    runtime_error("Assets.loadFiles table values must be strings")
                })?;
                requested_names.push(name);
            }

            let mut available = BTreeMap::new();
            let mut missing = Vec::new();
            {
                let known = load_asset_names
                    .lock()
                    .expect("downloaded asset map lock poisoned");
                for requested in requested_names {
                    if let Some(filename) = known.get(&requested) {
                        available.insert(requested, filename.clone());
                    } else if app_data_path(&load_asset_root, &requested)
                        .is_ok_and(|path| path.is_file())
                    {
                        available.insert(requested.clone(), requested);
                    } else {
                        missing.push(requested);
                    }
                }
            }

            let assets: mlua::Table = game_environment(lua)?.get("Assets")?;
            if missing.is_empty() {
                load_asset_names
                    .lock()
                    .expect("downloaded asset map lock poisoned")
                    .extend(available.clone());
                let result = lua.create_table()?;
                for (requested, filename) in available {
                    result.raw_set(requested, filename)?;
                }
                if let Value::Function(callback) = assets.get::<Value>("onLoadSuccess")? {
                    callback.call::<()>(result)?;
                }
            } else {
                let failed = lua.create_table()?;
                for (index, filename) in missing.into_iter().enumerate() {
                    failed.raw_set(index + 1, filename)?;
                }
                if let Value::Function(callback) = assets.get::<Value>("onLoadError")? {
                    callback.call::<()>((failed, 1_i32, "offline asset unavailable"))?;
                }
            }
            Ok(())
        })?,
    )?;
    let downloadable_sheet_resources = Arc::clone(&resource_runtime);
    let downloadable_sheet_root = Arc::clone(&data_root);
    downloadable_assets.set(
        "createSpriteSheet",
        lua.create_function(
            move |_, (name, descriptor, texture): (String, String, String)| {
                // sub_1000AC660 constructs the sheet from the descriptor and
                // texture paths completely before sub_100457724 transactionally
                // replaces the shared Resources map entry under `name`.
                let descriptor_path = app_data_path(&downloadable_sheet_root, &descriptor)
                    .ok()
                    .filter(|path| path.is_file())
                    .or_else(|| resolve_data_file(&downloadable_sheet_root, &descriptor).ok())
                    .ok_or_else(|| runtime_error(format!("asset was not found: {descriptor}")))?;
                let mut sheet = load_sprite_sheet_path(&descriptor_path, &descriptor)?;
                sheet.textures.clear();
                sheet.textures.push(texture);
                sheet.sprite_texture_indices.fill(0);
                let mut resources = downloadable_sheet_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                resources.replace_sprite_sheet_value(&name, sheet);
                resources.sprite_sheets.insert(name.clone());
                resources
                    .sprite_sheet_paths
                    .insert(name.clone(), descriptor);
                resources
                    .sprite_sheet_descriptor_paths
                    .insert(name.clone(), descriptor_path);
                resources.cache_sprite_sheet_host_bindings(&name, &downloadable_sheet_root);
                Ok(())
            },
        )?,
    )?;
    globals.set("Assets", downloadable_assets)?;
    Ok(())
}
