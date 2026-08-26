//! Locale selection and native bitmap/system-font metric bindings.

use crate::*;
use stella_assets::ka3d::BitmapFont;

pub(crate) fn install_selection(
    lua: &Lua,
    resource_api: &mlua::Table,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let load_locale_runtime = Arc::clone(&locale_runtime);
    resource_api.set(
        "loadLocale",
        lua.create_function(move |_, args: MultiValue| {
            // LuaResources' std::string,std::string dispatcher reads two
            // exact STRING slots and ignores additional stack values.
            let group = native_required_string(&args, 0, "res.loadLocale")?;
            let locale = native_required_string(&args, 1, "res.loadLocale")?;
            let table = {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                if !resources.text_group_sets.contains(&group) {
                    return Ok(());
                }
                resources
                    .text_group_set_tables
                    .get(&group)
                    .and_then(Option::as_ref)
                    .cloned()
            };
            {
                let mut locales = load_locale_runtime
                    .lock()
                    .expect("locale runtime lock poisoned");
                for groups in locales.loaded.values_mut() {
                    groups.remove(&group);
                }
            }
            let Some(loaded) = table
                .as_ref()
                .and_then(|table| localized_string_groups_from_table(table, &locale))
            else {
                return Err(runtime_error(format!(
                    "Trying to load TextGroup for language not present in data file. Language: \"{locale}\""
                )));
            };
            let mut locales = load_locale_runtime
                .lock()
                .expect("locale runtime lock poisoned");
            for (loaded_locale, strings) in loaded {
                locales
                    .loaded
                    .entry(loaded_locale)
                    .or_default()
                    .insert(group.clone(), strings);
            }
            Ok(())
        })?,
    )?;
    let use_locale_runtime = Arc::clone(&locale_runtime);
    resource_api.set(
        "useLocale",
        lua.create_function(move |_, args: MultiValue| {
            // Shared LuaResources std::string dispatcher requires exact
            // STRING slot one and does not enforce an exact arity.
            let locale = native_required_string(&args, 0, "res.useLocale")?;
            use_locale_runtime
                .lock()
                .expect("locale runtime lock poisoned")
                .current = locale;
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(crate) fn install_get_string(
    lua: &Lua,
    resource_api: &mlua::Table,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "getString",
        lua.create_function(move |_, args: MultiValue| {
            let group = native_required_string(&args, 0, "getString")?;
            let key = native_required_string(&args, 1, "getString")?;
            resolve_localized_string(&resource_runtime, &locale_runtime, &group, &key)
        })?,
    )
}

/// `ResourceManager::getString` (`sub_10045C380`) is also called internally
/// by both 2D and 3D drawString paths. Keep one implementation so those
/// render adapters cannot silently diverge from the public query member.
pub(crate) fn resolve_localized_string(
    resource_runtime: &Arc<Mutex<ResourceRuntime>>,
    locale_runtime: &Arc<Mutex<LocaleRuntime>>,
    group: &str,
    key: &str,
) -> LuaResult<String> {
    let group_exists = {
        let resources = resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        resources.text_group_sets.contains(group)
    };
    if !group_exists {
        return Ok(key.to_owned());
    }

    // ResourceManager::getString (`sub_10045C380`) walks the retained
    // TextGroupSet node and then asks its current TextGroup for the key.  It
    // never copies the source TEXT table on a lookup.  Keep the two host
    // mutexes non-overlapping, but likewise borrow the retained table only on
    // the exceptional present-vs-unloaded diagnostic path.
    let strings = locale_runtime.lock().expect("locale runtime lock poisoned");
    if let Some(value) = strings
        .loaded
        .get(&strings.current)
        .and_then(|locale| locale.get(group))
        .and_then(|group| group.get(key))
        .cloned()
    {
        return Ok(value);
    }
    if strings
        .loaded
        .get(&strings.current)
        .is_some_and(|locale| locale.contains_key(group))
    {
        return Ok(key.to_owned());
    }
    let current = strings.current.clone();
    drop(strings);
    let present = resource_runtime
        .lock()
        .expect("resource runtime lock poisoned")
        .text_group_set_tables
        .get(group)
        .and_then(Option::as_ref)
        .is_some_and(|table| localization_table_has_locale(table, &current));
    let reason = if present {
        "which is not loaded"
    } else {
        "not present in data file"
    };
    Err(runtime_error(format!(
        "Trying to get TextGroup for language {reason}. Language: \"{current}\""
    )))
}

pub(crate) fn install_get_locale(
    lua: &Lua,
    resource_api: &mlua::Table,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "getLocale",
        lua.create_function(move |_, ()| {
            Ok(locale_runtime
                .lock()
                .expect("locale runtime lock poisoned")
                .current
                .clone())
        })?,
    )
}

pub(crate) fn install_available_system_fonts(
    lua: &Lua,
    resource_api: &mlua::Table,
) -> LuaResult<()> {
    resource_api.set(
        "getAvailableSystemFonts",
        lua.create_function(move |lua, ()| {
            let table = lua.create_table()?;
            for (index, name) in platform_system_font_names().iter().enumerate() {
                table.raw_set(index + 1, name.as_str())?;
            }
            Ok(table)
        })?,
    )
}

pub(crate) fn install_metrics(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    bitmap_font_assets: Arc<BTreeMap<String, BitmapFont>>,
) -> LuaResult<()> {
    let width_font_resources = Arc::clone(&resource_runtime);
    let width_font_assets = Arc::clone(&bitmap_font_assets);
    resource_api.set(
        "getStringWidth",
        lua.create_function(move |_, args: MultiValue| {
            // The float(std::string) dispatcher uses sub_1005285CC for slot
            // one, ignores extras and publishes the member's float32 result.
            let text = native_required_string(&args, 0, "res.getStringWidth")?;
            let resources = width_font_resources
                .lock()
                .expect("resource runtime lock poisoned");
            let current = resources
                .current_font
                .as_deref()
                .ok_or_else(|| runtime_error("No font is set while trying to get string width"))?;
            if let Some(font) = resources.system_fonts.get(current) {
                return Ok(f64::from(system_font_string_width(font, &text)));
            }
            if let Some(font) = resources.bitmap_font_values.get(current) {
                return Ok(f64::from(bitmap_font_string_width(font, &text)));
            }
            if let Some(font) = width_font_assets.get(current) {
                return Ok(f64::from(bitmap_font_string_width(font, &text)));
            }
            Ok(0.0)
        })?,
    )?;
    for (method, metric, missing_message) in [
        (
            "getFontMaxAscending",
            FontMetric::MaxAscending,
            "No font is set while trying to get font max ascending",
        ),
        (
            "getFontMaxDescending",
            FontMetric::MaxDescending,
            "No font is set while trying to get font max descending",
        ),
        (
            "getFontLeading",
            FontMetric::Leading,
            "No font is set while trying to get font leading",
        ),
        (
            "getFontTracking",
            FontMetric::Tracking,
            "No font is set while trying to get font tracking",
        ),
        (
            "getFontHeight",
            FontMetric::Height,
            "No font is set while trying to get font height!",
        ),
    ] {
        let metric_font_resources = Arc::clone(&resource_runtime);
        let metric_font_assets = Arc::clone(&bitmap_font_assets);
        resource_api.set(
            method,
            lua.create_function(move |_, ()| {
                let resources = metric_font_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                let current = resources
                    .current_font
                    .as_deref()
                    .ok_or_else(|| runtime_error(missing_message))?;
                if let Some(font) = resources.system_fonts.get(current) {
                    return Ok(system_font_metric(font, metric));
                }
                if let Some(font) = resources.bitmap_font_values.get(current) {
                    return Ok(f64::from(bitmap_font_metric(font, metric)));
                }
                if let Some(font) = metric_font_assets.get(current) {
                    return Ok(f64::from(bitmap_font_metric(font, metric)));
                }
                Ok(0.0)
            })?,
        )?;
    }
    Ok(())
}
