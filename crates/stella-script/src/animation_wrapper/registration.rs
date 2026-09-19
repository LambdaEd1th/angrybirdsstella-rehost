//! Ordered Lua method table registered by Purple AnimationWrapper constructor.

mod fallback;
mod playback;
mod queries;
mod resources;
mod scene;

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult, Table};

use crate::{AnimationRuntime, RenderBridge, ResourceRuntime};

pub(crate) struct RegistrationContext {
    pub(crate) animation_runtime: Arc<Mutex<AnimationRuntime>>,
    pub(crate) data_root: Arc<PathBuf>,
    pub(crate) render: Arc<Mutex<RenderBridge>>,
    pub(crate) resource_runtime: Arc<Mutex<ResourceRuntime>>,
    pub(crate) missing: Arc<Mutex<BTreeSet<String>>>,
}

/// Complete publication sequence of `AnimationWrapper::AnimationWrapper` at
/// `sub_10000EC80` (`0x10000ED90..0x10000F264`).
#[cfg(test)]
pub(crate) const REGISTERED_ANIMATION_METHODS: &[&str] = &[
    "loadFromBundle",
    "loadFromAppData",
    "close",
    "closeAll",
    "isPlaying",
    "start",
    "stop",
    "stopAll",
    "pause",
    "resume",
    "setSpeed",
    "seek",
    "setTranslation",
    "setRotation",
    "setScale",
    "update",
    "draw",
    "setPlaybackEvent",
    "containsEntity",
    "getEntityPosition",
    "getEntityWorldPosition",
    "getEntityScale",
    "getEntityWorldScale",
    "getEntityWorldTransform",
    "getEntityWorldBounds",
    "setSkin",
    "getActions",
    "setShader",
    "clearCache",
    "preloadFromBundle",
    "preloadFromAppData",
];

pub(crate) fn install(lua: &Lua, globals: &Table, context: RegistrationContext) -> LuaResult<()> {
    let RegistrationContext {
        animation_runtime,
        data_root,
        render,
        resource_runtime,
        missing,
    } = context;
    let animation_native = lua.create_table()?;
    let animation_callbacks = lua.create_table()?;

    // Hopper xrefs at 0x10000ED94..0x10000F268 expose this exact native
    // publication order. Keep it explicit even though ordinary Lua tables do
    // not currently observe assignment order.
    resources::install_loads(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        Arc::clone(&data_root),
        Arc::clone(&resource_runtime),
    )?;
    resources::install_closing_with_resources(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        animation_callbacks.clone(),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    playback::install_controls_with_resources(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    scene::install_transforms(lua, &animation_native, Arc::clone(&animation_runtime))?;
    playback::install_update_with_resources(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        animation_callbacks.clone(),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    scene::install_draw(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        render,
    )?;
    playback::install_callback(lua, &animation_native, animation_callbacks)?;
    queries::install_entities(lua, &animation_native, Arc::clone(&animation_runtime))?;
    scene::install_skin(lua, &animation_native, Arc::clone(&animation_runtime))?;
    queries::install_actions(lua, &animation_native, Arc::clone(&animation_runtime))?;
    scene::install_shader(
        lua,
        &animation_native,
        Arc::clone(&animation_runtime),
        resource_runtime,
    )?;
    resources::install_cache(lua, &animation_native, animation_runtime, data_root)?;
    fallback::publish(lua, globals, animation_native, missing)
}
