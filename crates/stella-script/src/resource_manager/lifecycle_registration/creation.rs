//! Resource creation members at `sub_100446E2C`..`sub_1004477F8`.

use crate::resource_manager::system_font_color_from_lua;
use crate::*;

use super::loading::{
    load_bitmap_font_source, load_composite_set_source, load_text_group_set_source,
};

fn optional_boolean(args: &MultiValue, index: usize, default: bool) -> bool {
    match args.iter().nth(index) {
        Some(Value::Boolean(value)) => *value,
        _ => default,
    }
}

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let path_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "setPath",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "setPath")?;
            path_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .path = resource_normalized_path(&path);
            Ok(())
        })?,
    )?;

    let sprite_resources = Arc::clone(&resource_runtime);
    let sprite_data_root = Arc::clone(&data_root);
    resource_api.set(
        "createSpriteSheet",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "createSpriteSheet")?;
            let replace = optional_boolean(&args, 1, false);
            // The adapter probes argument 3 independently, defaults it to
            // true, then leaves X3 unused when calling sub_100457E38.
            let _unused_texture_flag = optional_boolean(&args, 2, true);
            create_sprite_sheet(&sprite_resources, &sprite_data_root, &path, replace)?;
            Ok(())
        })?,
    )?;

    let composite_resources = Arc::clone(&resource_runtime);
    let composite_data_root = Arc::clone(&data_root);
    resource_api.set(
        "createCompoSpriteSet",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "createCompoSpriteSet")?;
            let replace = optional_boolean(&args, 1, false);
            let key = resource_double_file_stem(&path);
            let resolved = {
                let resources = composite_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !replace && resources.composite_sets.contains(&key) {
                    return Ok(());
                }
                resource_join_path(&resources.path, &path)
            };
            // sub_1004586AC also loads transactionally, but an empty parsed
            // set is a successful no-op which preserves an existing value.
            let Some(set) = load_composite_set_source(&composite_data_root, &resolved)? else {
                return Ok(());
            };
            let mut resources = composite_resources
                .lock()
                .expect("resource runtime lock poisoned");
            let regions = resources
                .bind_composite_set_regions(&set, &composite_data_root, &resolved)
                .map_err(runtime_error)?;
            resources.replace_composite_set_value(&key, set, regions);
            resources.composite_sets.insert(key.clone());
            resources.composite_set_paths.insert(key, resolved);
            Ok(())
        })?,
    )?;

    let bitmap_resources = Arc::clone(&resource_runtime);
    let bitmap_data_root = Arc::clone(&data_root);
    resource_api.set(
        "createBitmapFont",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "createBitmapFont")?;
            let replace = optional_boolean(&args, 1, false);
            let key = resource_file_stem(&path);
            let resolved = {
                let resources = bitmap_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !replace
                    && (resources.bitmap_fonts.contains(&key)
                        || resources.system_fonts.contains_key(&key))
                {
                    return Ok(());
                }
                resource_join_path(&resources.path, &path)
            };
            let descriptor_path =
                resolve_data_file(&bitmap_data_root, &resolved).map_err(runtime_error)?;
            // sub_10042A5B0 finishes the binary FONT constructor before the
            // shared IFont map node is overwritten.
            let font = load_bitmap_font_source(&bitmap_data_root, &resolved)?;
            let mut resources = bitmap_resources
                .lock()
                .expect("resource runtime lock poisoned");
            // Bitmap and system fonts occupy the same native IFont map.
            resources.remove_system_font(&key);
            resources.bitmap_fonts.insert(key.clone());
            resources.bitmap_font_paths.insert(key.clone(), resolved);
            resources
                .bitmap_font_descriptor_paths
                .insert(key.clone(), descriptor_path);
            resources.bitmap_font_values.insert(key, font);
            Ok(())
        })?,
    )?;

    install_system_font(lua, resource_api, Arc::clone(&resource_runtime))?;
    install_stroked_system_font(lua, resource_api, Arc::clone(&resource_runtime))?;

    let text_locale_runtime = Arc::clone(&locale_runtime);
    let text_data_root = Arc::clone(&data_root);
    resource_api.set(
        "createTextGroupSet",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "createTextGroupSet")?;
            let replace = optional_boolean(&args, 1, false);
            let key = resource_file_stem(&path);
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            if !replace && resources.text_group_sets.contains(&key) {
                return Ok(());
            }
            let resolved = resource_join_path(&resources.path, &path);
            // sub_100459AD8 is deliberately non-transactional: operator[]
            // and shared-pointer assignment destroy the previous object before
            // sub_1004729A4 parses the TEXT payload.
            resources.text_group_sets.insert(key.clone());
            resources
                .text_group_set_paths
                .insert(key.clone(), resolved.clone());
            resources.text_group_set_tables.insert(key.clone(), None);
            drop(resources);
            let mut locales = text_locale_runtime
                .lock()
                .expect("locale runtime lock poisoned");
            for groups in locales.loaded.values_mut() {
                groups.remove(&key);
            }
            drop(locales);
            let table = load_text_group_set_source(&text_data_root, &resolved)?;
            resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .text_group_set_tables
                .insert(key, Some(table));
            Ok(())
        })?,
    )?;
    Ok(())
}

/// Invoke the `sub_100457E38` member shared by LuaResources and the older
/// global ResourceManager facade. The return value distinguishes a real load
/// from the replace=false existing-object fast path.
pub(crate) fn create_sprite_sheet(
    resource_runtime: &Arc<Mutex<ResourceRuntime>>,
    data_root: &Path,
    path: &str,
    replace: bool,
) -> LuaResult<bool> {
    let key = resource_double_file_stem(path);
    let resolved = {
        let resources = resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        if !replace && resources.sprite_sheets.contains(&key) {
            return Ok(false);
        }
        resource_join_path(&resources.path, path)
    };
    // sub_100457E38 completely loads the candidate before removing the old
    // sheet's global sprite registrations or map pointer.
    let descriptor_path = resolve_data_file(data_root, &resolved).map_err(runtime_error)?;
    let sheet = load_sprite_sheet_path(&descriptor_path, &resolved)?;
    let mut resources = resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    resources.replace_sprite_sheet_value(&key, sheet);
    resources.sprite_sheets.insert(key.clone());
    resources.sprite_sheet_paths.insert(key.clone(), resolved);
    resources
        .sprite_sheet_descriptor_paths
        .insert(key.clone(), descriptor_path);
    resources.cache_sprite_sheet_host_bindings(&key, data_root);
    resources.released_sprite_sheet_resources.remove(&key);
    Ok(true)
}

fn install_system_font(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "createSystemFont",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "createSystemFont")?;
            let family = native_required_string(&args, 1, "createSystemFont")?;
            let mut numbers = [0.0_f32; 5];
            for (offset, target) in numbers.iter_mut().enumerate() {
                *target = native_required_number(&args, offset + 2, "createSystemFont")? as f32;
            }
            let mut cursor = 7;
            let style = match args.iter().nth(cursor).and_then(value_number) {
                Some(style) => {
                    cursor += 1;
                    style as f32
                }
                None => 0.0,
            };
            let replace = optional_boolean(&args, cursor, false);
            let label_pool_epoch = {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !replace
                    && (resources.system_fonts.contains_key(&name)
                        || resources.bitmap_fonts.contains(&name))
                {
                    return Ok(());
                }
                resources.system_font_label_pool_epoch
            };
            // sub_1004471B4 packs the Lua slots as A,R,G,B before constructing
            // gr::Color; they are not the more usual R,G,B,A ordering.
            let fill = system_font_color_from_lua([numbers[1], numbers[2], numbers[3], numbers[4]]);
            let font = create_system_font_state(
                label_pool_epoch,
                &family,
                f64::from(numbers[0]),
                fill,
                0.0,
                [0, 0, 0, 255],
                f64::from(style),
            )?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            resources.bitmap_fonts.remove(&name);
            resources.bitmap_font_paths.remove(&name);
            resources.bitmap_font_descriptor_paths.remove(&name);
            resources.bitmap_font_values.remove(&name);
            resources.system_fonts.insert(name, font);
            Ok(())
        })?,
    )
}

fn install_stroked_system_font(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "createSystemFontWithStroke",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "createSystemFontWithStroke")?;
            let family = native_required_string(&args, 1, "createSystemFontWithStroke")?;
            let mut numbers = [0.0_f32; 11];
            for (offset, target) in numbers.iter_mut().enumerate() {
                *target =
                    native_required_number(&args, offset + 2, "createSystemFontWithStroke")? as f32;
            }
            let replace = optional_boolean(&args, 13, false);
            let label_pool_epoch = {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !replace
                    && (resources.system_fonts.contains_key(&name)
                        || resources.bitmap_fonts.contains(&name))
                {
                    return Ok(());
                }
                resources.system_font_label_pool_epoch
            };
            // sub_100447480 uses A,R,G,B for both colors; style and stroke
            // width occupy the two scalar slots between them.
            let fill = system_font_color_from_lua([numbers[1], numbers[2], numbers[3], numbers[4]]);
            let stroke =
                system_font_color_from_lua([numbers[7], numbers[8], numbers[9], numbers[10]]);
            let font = create_system_font_state(
                label_pool_epoch,
                &family,
                f64::from(numbers[0]),
                fill,
                f64::from(numbers[6]),
                stroke,
                f64::from(numbers[5]),
            )?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            resources.bitmap_fonts.remove(&name);
            resources.bitmap_font_paths.remove(&name);
            resources.bitmap_font_descriptor_paths.remove(&name);
            resources.bitmap_font_values.remove(&name);
            resources.system_fonts.insert(name, font);
            Ok(())
        })?,
    )
}
