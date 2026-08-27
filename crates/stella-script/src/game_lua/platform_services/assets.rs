//! Downloadable Assets service and named sprite-sheet lifetime.

use crate::*;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
enum Completion {
    Success(BTreeMap<String, String>),
    Error {
        failed: Vec<String>,
        code: i32,
        message: String,
    },
}

/// Main-thread completion state retained by Purple's native Assets object.
#[derive(Clone, Debug, Default)]
pub(crate) struct AssetsRuntime {
    downloaded_asset_names: Arc<Mutex<BTreeMap<String, String>>>,
    completions: Arc<Mutex<VecDeque<Completion>>>,
}

impl AssetsRuntime {
    fn push(&self, completion: Completion) {
        self.completions
            .lock()
            .expect("assets completion queue lock poisoned")
            .push_back(completion);
    }

    fn take_pending(&self) -> VecDeque<Completion> {
        std::mem::take(
            &mut *self
                .completions
                .lock()
                .expect("assets completion queue lock poisoned"),
        )
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<AssetsRuntime> {
    let runtime = AssetsRuntime::default();
    let downloadable_assets = lua.create_table()?;
    // Purple's Assets constructor sub_1000AC118 publishes only loadFiles and
    // createSpriteSheet. getAssetFilename/haveBeenDownloaded are defined by
    // scripts_common/cloud/rovioid/Assets.lua after the native table exists.
    let load_asset_root = Arc::clone(&data_root);
    let load_runtime = runtime.clone();
    downloadable_assets.set(
        "loadFiles",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000AD28C -> sub_1000AD2F4 requires an
            // exact table in slot one and never checks the remaining stack.
            let requested = native_required_table(&args, 0, "Assets.loadFiles")?;
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
                let known = load_runtime
                    .downloaded_asset_names
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

            if missing.is_empty() {
                load_runtime.push(Completion::Success(available));
            } else {
                load_runtime.push(Completion::Error {
                    failed: missing,
                    code: 1,
                    message: "offline asset unavailable".to_owned(),
                });
            }
            Ok(())
        })?,
    )?;
    let downloadable_sheet_resources = Arc::clone(&resource_runtime);
    let downloadable_sheet_root = Arc::clone(&data_root);
    downloadable_assets.set(
        "createSpriteSheet",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000ACDE8 delegates to sub_1000ACE50,
            // which reads all three slots through the exact STRING-tag
            // accessor sub_1005285CC and ignores later stack values.
            let name = native_required_string(&args, 0, "Assets.createSpriteSheet")?;
            let descriptor = native_required_string(&args, 1, "Assets.createSpriteSheet")?;
            let texture = native_required_string(&args, 2, "Assets.createSpriteSheet")?;
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
        })?,
    )?;
    globals.set("Assets", downloadable_assets)?;
    Ok(runtime)
}

/// Deliver RCS Assets request functors on the application thread.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &AssetsRuntime) -> LuaResult<()> {
    // The completion functors at sub_1000AC964/sub_1000ACA0C retain the
    // native Assets LuaObject created by sub_1000AC118. The shipped facade is
    // a separate GameLua-environment table and installs its callbacks onto
    // this root object through `_G.Assets`.
    let native_assets = lua.globals().get::<mlua::Table>("Assets")?;
    // Take a frame-head snapshot. A callback that starts another request must
    // not complete recursively in the same dispatcher pass: Purple submits a
    // fresh asynchronous Func5 job for every call.
    for completion in runtime.take_pending() {
        match completion {
            Completion::Success(available) => {
                runtime
                    .downloaded_asset_names
                    .lock()
                    .expect("downloaded asset map lock poisoned")
                    .extend(available.clone());
                let result = lua.create_table()?;
                for (requested, filename) in available {
                    result.raw_set(requested, filename)?;
                }
                native_assets
                    .get::<mlua::Function>("onLoadSuccess")?
                    .call::<()>(result)?;
            }
            Completion::Error {
                failed,
                code,
                message,
            } => {
                let failed_table = lua.create_table()?;
                for (index, filename) in failed.into_iter().enumerate() {
                    failed_table.raw_set(index + 1, filename)?;
                }
                native_assets
                    .get::<mlua::Function>("onLoadError")?
                    .call::<()>((failed_table, code, message))?;
            }
        }
    }
    Ok(())
}
