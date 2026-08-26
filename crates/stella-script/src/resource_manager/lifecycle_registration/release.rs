//! Resource release members at `sub_100447FDC`..`sub_1004481C4`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
) -> LuaResult<()> {
    let sprite_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "releaseSpriteSheet",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "releaseSpriteSheet")?;
            let release_resources = if args.len() >= 2 {
                native_required_boolean(&args, 1, "releaseSpriteSheet")?
            } else {
                false
            };
            release_sprite_sheet(&sprite_resources, &path, release_resources);
            Ok(())
        })?,
    )?;

    let composite_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "releaseCompoSpriteSet",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "releaseCompoSpriteSet")?;
            let key = resource_double_file_stem(&path);
            let mut resources = composite_resources
                .lock()
                .expect("resource runtime lock poisoned");
            resources.remove_composite_set_value(&key);
            resources.composite_sets.remove(&key);
            resources.composite_set_paths.remove(&key);
            Ok(())
        })?,
    )?;

    let font_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "releaseFont",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "releaseFont")?;
            let key = resource_file_stem(&name);
            let mut resources = font_resources
                .lock()
                .expect("resource runtime lock poisoned");
            resources.bitmap_fonts.remove(&key);
            resources.bitmap_font_paths.remove(&key);
            resources.bitmap_font_descriptor_paths.remove(&key);
            resources.bitmap_font_texture_sources.remove(&key);
            resources.bitmap_font_values.remove(&key);
            resources.remove_system_font(&key);
            if resources.current_font.as_deref() == Some(key.as_str()) {
                resources.current_font = None;
            }
            Ok(())
        })?,
    )?;

    let text_resources = Arc::clone(&resource_runtime);
    let text_locales = Arc::clone(&locale_runtime);
    resource_api.set(
        "releaseTextGroupSet",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "releaseTextGroupSet")?;
            let key = resource_file_stem(&name);
            let mut resources = text_resources
                .lock()
                .expect("resource runtime lock poisoned");
            let removed = resources.text_group_sets.remove(&key);
            resources.text_group_set_paths.remove(&key);
            resources.text_group_set_tables.remove(&key);
            drop(resources);
            if removed {
                let mut locales = text_locales.lock().expect("locale runtime lock poisoned");
                for groups in locales.loaded.values_mut() {
                    groups.remove(&key);
                }
            }
            Ok(())
        })?,
    )?;

    resource_api.set(
        "releaseAudio",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "releaseAudio")?;
            resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_clips
                .remove(&name);
            let mut audio = audio_runtime.lock().expect("audio runtime lock poisoned");
            audio.clips.retain(|_, clip| clip.name != name);
            audio.composite_clips.remove(&name);
            audio.assets.remove(&name);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(crate) fn release_sprite_sheet(
    resource_runtime: &Arc<Mutex<ResourceRuntime>>,
    path: &str,
    release_resources: bool,
) {
    let key = resource_double_file_stem(path);
    let mut resources = resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    if resources.sprite_sheets.contains(&key) {
        if release_resources {
            // sub_10045ABC4 always unregisters the sheet's resources through
            // sub_1004578BC. Its boolean=true branch then clears the retained
            // SpriteSheet resource pointer instead of erasing the map node.
            resources.deactivate_sprite_sheet_value(&key);
            resources.released_sprite_sheet_resources.insert(key);
        } else {
            resources.remove_sprite_sheet_value(&key);
            resources.sprite_sheets.remove(&key);
            resources.sprite_sheet_paths.remove(&key);
            resources.sprite_sheet_descriptor_paths.remove(&key);
            resources.released_sprite_sheet_resources.remove(&key);
        }
    }
}
