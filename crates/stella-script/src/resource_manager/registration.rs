//! Lua method tables registered by Purple game::LuaResources constructor.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult, Table, Value};
use stella_assets::ka3d::BitmapFont;

use super::{
    audio_playback_registration, audio_setup_registration, draw_registration,
    legacy_registration::install as install_legacy_resource_manager, lifecycle_registration,
    locale_font_registration, query_registration,
};
use crate::*;

pub(crate) struct RegistrationContext {
    pub(crate) missing: Arc<Mutex<BTreeSet<String>>>,
    pub(crate) render: Arc<Mutex<RenderBridge>>,
    pub(crate) resource_runtime: Arc<Mutex<ResourceRuntime>>,
    pub(crate) locale_runtime: Arc<Mutex<LocaleRuntime>>,
    pub(crate) audio_runtime: Arc<Mutex<AudioRuntime>>,
    pub(crate) data_root: Arc<PathBuf>,
    pub(crate) bitmap_font_assets: Arc<BTreeMap<String, BitmapFont>>,
}

/// Complete method-name inventory published by `game::LuaResources` at
/// `sub_100446570` (`0x1004465D8..0x100446D98`).  Kept next to the ordered
/// coordinator so regressions cannot be hidden by the diagnostic `__index`.
#[cfg(test)]
pub(crate) const REGISTERED_RESOURCE_METHODS: &[&str] = &[
    "setPath",
    "createSpriteSheet",
    "createCompoSpriteSet",
    "createBitmapFont",
    "createSystemFont",
    "createSystemFontWithStroke",
    "createTextGroupSet",
    "createAudioOutput",
    "createAudioInput",
    "createAudio",
    "createCompositeAudio",
    "captureSprite",
    "releaseSpriteSheet",
    "releaseCompoSpriteSet",
    "releaseFont",
    "releaseTextGroupSet",
    "releaseAudio",
    "loadLocale",
    "useLocale",
    "useFont",
    "getAvailableSystemFonts",
    "drawSprite",
    "drawCompoSprite",
    "drawString",
    "setClipRect",
    "getClipRect",
    "getString",
    "playAudio",
    "stopAudio",
    "stopAllAudio",
    "isAudioPlaying",
    "getSpriteBounds",
    "getSpritePivot",
    "getCompoSpriteBounds",
    "getCompoSpriteData",
    "getCompoSpriteEntry",
    "setCompoSpriteEntry",
    "getStringWidth",
    "getFontMaxAscending",
    "getFontMaxDescending",
    "getFontLeading",
    "getFontTracking",
    "getFontHeight",
    "getLocale",
    "startAudioOutput",
    "stopAudioOutput",
    "startAudioInput",
    "stopAudioInput",
    "setMasterVolume",
    "setTrackVolume",
    "getTrackVolume",
    "openURL",
];

/// Complete native `ResourceManager` table from `sub_100093904`.
#[cfg(test)]
pub(crate) const REGISTERED_LEGACY_RESOURCE_METHODS: &[&str] = &[
    "native_createSpriteSheet",
    "native_releaseSpriteSheet",
    "native_createAudio",
    "native_createAudioFromAppData",
    "native_releaseAudio",
    "native_playAudio",
];

pub(crate) fn install(lua: &Lua, globals: &Table, context: RegistrationContext) -> LuaResult<()> {
    let RegistrationContext {
        missing,
        render,
        resource_runtime,
        locale_runtime,
        audio_runtime,
        data_root,
        bitmap_font_assets,
    } = context;
    let resource_api = lua.create_table()?;
    let resource_metatable = lua.create_table()?;
    let missing_resource_methods = Arc::clone(&missing);
    resource_metatable.set(
        "__index",
        lua.create_function(move |_, (_table, key): (mlua::Table, String)| {
            missing_resource_methods
                .lock()
                .expect("missing-global lock poisoned")
                .insert(format!("res.{key}"));
            Ok(Value::Nil)
        })?,
    )?;
    resource_api.set_metatable(Some(resource_metatable))?;
    // Exact publication order in game::LuaResources::LuaResources at
    // sub_100446570. The phases remain separate because the constructor
    // interleaves creation/release, locale, draw, query and audio owners.
    lifecycle_registration::install_creation(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&locale_runtime),
        Arc::clone(&data_root),
    )?;
    audio_setup_registration::install_creation(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
        Arc::clone(&data_root),
    )?;
    draw_registration::install_capture(lua, &resource_api, Arc::clone(&render))?;
    lifecycle_registration::install_release(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
        Arc::clone(&locale_runtime),
    )?;
    locale_font_registration::install_selection(
        lua,
        &resource_api,
        Arc::clone(&locale_runtime),
        Arc::clone(&resource_runtime),
    )?;
    query_registration::install_use_font(lua, &resource_api, Arc::clone(&resource_runtime))?;
    locale_font_registration::install_available_system_fonts(lua, &resource_api)?;
    draw_registration::install_draw(
        lua,
        &resource_api,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&locale_runtime),
        Arc::clone(&data_root),
    )?;
    query_registration::install_clip_rect(
        lua,
        &resource_api,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
    )?;
    locale_font_registration::install_get_string(
        lua,
        &resource_api,
        Arc::clone(&locale_runtime),
        Arc::clone(&resource_runtime),
    )?;
    audio_playback_registration::install_resource_playback(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?;
    query_registration::install_geometry(
        lua,
        &resource_api,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
    )?;
    locale_font_registration::install_metrics(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&bitmap_font_assets),
    )?;
    locale_font_registration::install_get_locale(lua, &resource_api, Arc::clone(&locale_runtime))?;
    audio_setup_registration::install_controls(lua, &resource_api, Arc::clone(&resource_runtime))?;
    audio_playback_registration::install_volume(
        lua,
        &resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?;
    draw_registration::install_open_url_and_publish(
        lua,
        globals,
        &resource_api,
        Arc::clone(&render),
    )?;

    install_legacy_resource_manager(
        lua,
        globals,
        Arc::clone(&missing),
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
        Arc::clone(&data_root),
    )?;
    Ok(())
}
