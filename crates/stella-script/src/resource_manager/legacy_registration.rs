//! Legacy global `ResourceManager` table recovered from `sub_100093904`.

use crate::*;

use super::{legacy_usage, lifecycle_registration, native_play_audio};

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    missing: Arc<Mutex<BTreeSet<String>>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let resource_manager = lua.create_table()?;
    let create_sheet_resources = Arc::clone(&resource_runtime);
    let create_sheet_root = Arc::clone(&data_root);
    resource_manager.set(
        "native_createSpriteSheet",
        lua.create_function(move |lua, args: MultiValue| {
            let path = native_required_string(&args, 0, "native_createSpriteSheet")?;
            let textures = legacy_usage::sprite_sheet_textures(&create_sheet_root, &path);
            // ResourceManager::native_createSpriteSheet at sub_10009470C
            // forwards to LuaResources::createSpriteSheet with replace=false
            // and the otherwise-unused texture flag true.
            let newly_loaded = lifecycle_registration::create_sprite_sheet(
                &create_sheet_resources,
                &create_sheet_root,
                &path,
                false,
            )?;
            let published = {
                let mut resources = create_sheet_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !newly_loaded {
                    None
                } else {
                    let mut upload_delta = 0u32;
                    let mut retained = Vec::with_capacity(textures.len());
                    for texture in textures {
                        let count = resources
                            .legacy_texture_ref_counts
                            .entry(texture.cache_key.clone())
                            .or_default();
                        if *count == 0 {
                            upload_delta = upload_delta.wrapping_add(texture.uploaded_bytes);
                        }
                        *count = count.wrapping_add(1);
                        retained.push(texture.cache_key);
                    }
                    resources
                        .legacy_sheet_textures
                        .insert(path.clone(), retained);
                    if upload_delta == 0 {
                        None
                    } else {
                        resources.legacy_texture_usage.insert(path, upload_delta);
                        Some(native_sum(resources.legacy_texture_usage.values()))
                    }
                }
            };
            if let Some(bytes) = published {
                publish_memory_global(lua, "g_usedTextureMemory", bytes)?;
            }
            Ok(())
        })?,
    )?;
    let release_sheet_resources = Arc::clone(&resource_runtime);
    resource_manager.set(
        "native_releaseSpriteSheet",
        lua.create_function(move |lua, args: MultiValue| {
            let path = native_required_string(&args, 0, "native_releaseSpriteSheet")?;
            // sub_100094800 forwards releaseResources=false before updating
            // its private per-path memory counter tree.
            lifecycle_registration::release_sprite_sheet(&release_sheet_resources, &path, false);
            let total = {
                let mut resources = release_sheet_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                if let Some(textures) = resources.legacy_sheet_textures.remove(&path) {
                    for texture in textures {
                        let remove = resources
                            .legacy_texture_ref_counts
                            .get_mut(&texture)
                            .map(|count| {
                                *count = count.saturating_sub(1);
                                *count == 0
                            })
                            .unwrap_or(false);
                        if remove {
                            resources.legacy_texture_ref_counts.remove(&texture);
                        }
                    }
                }
                // sub_100094800 uses map operator[], retaining a zero-valued
                // node even if the path was never loaded.
                resources.legacy_texture_usage.insert(path, 0);
                native_sum(resources.legacy_texture_usage.values())
            };
            publish_memory_global(lua, "g_usedTextureMemory", total)?;
            Ok(())
        })?,
    )?;
    for method in ["native_createAudio", "native_createAudioFromAppData"] {
        let resources = Arc::clone(&resource_runtime);
        let audio_assets = Arc::clone(&audio_runtime);
        let audio_root = Arc::clone(&data_root);
        let from_app_data = method == "native_createAudioFromAppData";
        resource_manager.set(
            method,
            lua.create_function(move |lua, args: MultiValue| {
                // sub_100093C10/sub_10009410C strictly read the two strings,
                // default argument 3 to true only when it is absent, and key
                // both lifetime maps by the logical second string.
                let path = args
                    .front()
                    .and_then(value_string)
                    .ok_or_else(|| runtime_error("native_createAudio argument 1 must be string"))?;
                let name =
                    args.iter().nth(1).and_then(value_string).ok_or_else(|| {
                        runtime_error("native_createAudio argument 2 must be string")
                    })?;
                let streaming = if args.len() >= 3 {
                    match args.iter().nth(2) {
                        Some(Value::Boolean(value)) => *value,
                        _ => {
                            return Err(runtime_error(
                                "native_createAudio argument 3 must be boolean",
                            ));
                        }
                    }
                } else {
                    true
                };
                let source_path = legacy_usage::audio_file_path(&audio_root, &path, from_app_data)
                    .ok_or_else(|| runtime_error(format!("Failed to open {path}")))?;
                let file_info =
                    legacy_usage::audio_file_info(&source_path, streaming).ok_or_else(|| {
                        runtime_error(format!(
                            "Unsupported audio file format while reading {}",
                            source_path.display()
                        ))
                    })?;
                let decoded_bytes = file_info.resident_bytes;
                let published = {
                    let mut resources = resources.lock().expect("resource runtime lock poisoned");
                    resources.audio_clips.insert(name.clone());
                    resources.legacy_audio_play_counts.insert(name.clone(), 0);
                    decoded_bytes.map(|bytes| {
                        resources.legacy_audio_usage.insert(name.clone(), bytes);
                        native_sum(resources.legacy_audio_usage.values())
                    })
                };
                if let Some(bytes) = published {
                    publish_memory_global(lua, "g_usedAudioMemory", bytes)?;
                }
                let mut audio = audio_assets.lock().expect("audio runtime lock poisoned");
                audio.composite_clips.remove(&name);
                audio.replace_asset(
                    name,
                    Some(AudioAssetState {
                        source: file_info.source,
                        duration: file_info.duration,
                        sample_frames: file_info.sample_frames,
                    }),
                );
                Ok(())
            })?,
        )?;
    }
    let release_audio_resources = Arc::clone(&resource_runtime);
    let release_audio_playback = Arc::clone(&audio_runtime);
    resource_manager.set(
        "native_releaseAudio",
        lua.create_function(move |_, args: MultiValue| {
            let name = args
                .front()
                .and_then(value_string)
                .ok_or_else(|| runtime_error("native_releaseAudio argument 1 must be string"))?;
            release_audio_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_clips
                .remove(&name);
            let mut audio = release_audio_playback
                .lock()
                .expect("audio runtime lock poisoned");
            for clip in audio.clips.values_mut().filter(|clip| clip.name == name) {
                clip.finished = true;
            }
            audio.composite_clips.remove(&name);
            audio.assets.remove(&name);
            Ok(())
        })?,
    )?;
    resource_manager.set(
        "native_playAudio",
        // sub_100093B00 invokes LuaResources::playAudio (`sub_100448A94`)
        // with the same optional arguments, increments its private counter,
        // then deliberately returns zero Lua results.
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "native_playAudio")?;
            let volume = if args.len() < 2 {
                1.0
            } else {
                native_required_number(&args, 1, "native_playAudio")? as f32
            };
            let looping = if args.len() < 3 {
                false
            } else {
                native_required_boolean(&args, 2, "native_playAudio")?
            };
            let channel = if args.len() < 4 {
                0
            } else {
                native_fcvtzs_f32(native_required_number(&args, 3, "native_playAudio")? as f32)
            };
            let _handle = native_play_audio(
                &resource_runtime,
                &audio_runtime,
                name.clone(),
                volume,
                looping,
                channel,
            )?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            *resources.legacy_audio_play_counts.entry(name).or_default() += 1;
            Ok(())
        })?,
    )?;
    let manager_metatable = lua.create_table()?;
    let missing_manager_methods = Arc::clone(&missing);
    manager_metatable.set(
        "__index",
        lua.create_function(move |_, (_table, key): (mlua::Table, String)| {
            missing_manager_methods
                .lock()
                .expect("missing-global lock poisoned")
                .insert(format!("ResourceManager.{key}"));
            Ok(Value::Nil)
        })?,
    )?;
    resource_manager.set_metatable(Some(manager_metatable))?;
    globals.set("ResourceManager", resource_manager)?;
    Ok(())
}

/// The original loops accumulate signed 32-bit map values and then pass the
/// result through the float-only global setter at sub_10007E538.
fn native_sum<'a>(values: impl Iterator<Item = &'a u32>) -> u32 {
    values.fold(0u32, |total, value| total.wrapping_add(*value))
}

fn publish_memory_global(lua: &Lua, name: &str, bytes: u32) -> LuaResult<()> {
    lua.globals().set(name, f64::from(bytes as f32))
}
