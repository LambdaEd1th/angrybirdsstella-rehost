//! Animation scene loading, closing and parsed-asset cache bindings.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

mod attachment;

#[cfg(test)]
mod tests;

type AnimationResourceSnapshot = (
    BTreeMap<String, SpriteGeometry>,
    BTreeMap<String, NativeSpriteMetrics>,
    BTreeMap<String, SpriteCatalogRegion>,
);

pub(super) fn install_loads(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    for (method, from_bundle) in [("loadFromBundle", true), ("loadFromAppData", false)] {
        let runtime = Arc::clone(&animation_runtime);
        let resources = Arc::clone(&resource_runtime);
        let sprite_data_root = Arc::clone(&data_root);
        let animation_root = if from_bundle {
            Arc::clone(&data_root)
        } else {
            Arc::new(app_data_root(&data_root))
        };
        animation_native.set(
            method,
            lua.create_function(move |lua, args: MultiValue| {
                let tag = native_required_string(&args, 0, method)?;
                let filename = native_required_string(&args, 1, method)?;
                let asset = {
                    let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                    let cached = if from_bundle {
                        runtime.bundle_cache.get(&filename)
                    } else {
                        runtime.app_data_cache.get(&filename)
                    }
                    .cloned();
                    if let Some(asset) = cached {
                        asset
                    } else {
                        let cache = if from_bundle {
                            &mut runtime.bundle_json_cache
                        } else {
                            &mut runtime.app_data_json_cache
                        };
                        let document = cached_document(cache, &animation_root, &filename)?;
                        let skins = cached_document(
                            cache,
                            &animation_root,
                            &animation_skin_filename(&filename),
                        )?;
                        animation_asset_from_documents(&document, &skins)
                    }
                };
                if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                    eprintln!(
                        "animation-native load tag={tag} file={filename} actions={:?}",
                        asset.actions
                    );
                }
                let snapshot = animation_resource_snapshot(
                    &resources.lock().expect("resource runtime lock poisoned"),
                    &asset.definition,
                    &sprite_data_root,
                );
                let (previous, generation) = attachment::prepare(
                    &mut runtime.lock().expect("animation runtime lock poisoned"),
                    tag,
                    asset,
                    snapshot,
                );
                if let Some(previous) = previous {
                    post_animation_entity_removal(lua, previous)?;
                }
                post_animation_entity_attachment(lua, generation)?;
                // AnimationWrapper::load core calls `sub_10057C418` twice at
                // 0x10001076C/0x100010788 after installing the scene.
                dispatch_registered_application_events(lua)?;
                dispatch_registered_application_events(lua)?;
                // sub_10001D43C is the void two-string Lua adapter.
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn install_closing(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
) -> LuaResult<()> {
    install_closing_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        None,
        None,
    )
}

pub(super) fn install_closing_with_resources(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    install_closing_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        Some(resource_runtime),
        Some(data_root),
    )
}

fn install_closing_inner(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<PathBuf>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    let callbacks = animation_callbacks.clone();
    lua.set_named_registry_value(
        ANIMATION_ENTITY_ATTACHMENT_REGISTRY_KEY,
        lua.create_function(move |_, generation: u64| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            attachment::attach(&mut runtime, &callbacks, generation)
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let callbacks = animation_callbacks.clone();
    lua.set_named_registry_value(
        ANIMATION_ENTITY_REMOVAL_REGISTRY_KEY,
        lua.create_function(move |_, generation: u64| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            attachment::remove(&mut runtime, &callbacks, generation)
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "close",
        lua.create_function(move |lua, args: MultiValue| {
            let tag = native_required_string(&args, 0, "close")?;
            let generation = {
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                request_animation_close(&mut runtime, tag)
            };
            if let Some(generation) = generation {
                finish_animation_close(lua, generation)?;
            }
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "closeAll",
        lua.create_function(move |lua, _: MultiValue| {
            {
                let resources = resource_runtime
                    .as_ref()
                    .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                // sub_100012910 is stopAll: seek every live control to zero,
                // force its targets, and keep the entity tree queryable during
                // the first scheduler drain.
                super::playback::stop_all_native(
                    &mut runtime,
                    resources.as_deref(),
                    data_root.as_deref().map(PathBuf::as_path),
                );
            }
            dispatch_registered_application_events(lua)?;
            {
                // setRoot(nullptr), between 0x1000128B8 and 0x1000128D0,
                // removes even scenes loaded by the first drain. Wrapper
                // control/skin owners remain available until after the second.
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                runtime.root_present = false;
                runtime.shadow_scenes.clear();
                let tags = runtime.definitions.keys().cloned().collect::<Vec<_>>();
                for tag in tags {
                    remove_animation_scene(&mut runtime, &callbacks, &tag)?;
                }
            }
            dispatch_registered_application_events(lua)?;
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            let live = runtime.definitions.keys().cloned().collect::<BTreeSet<_>>();
            runtime.playback.retain(|tag, playback| {
                // A second-drain callback can create a new root and scene.
                // Erasing the wrapper map does not destroy that scene's
                // active controls or the values its targets have latched.
                playback.wrapper_control_present = false;
                playback.detached_current = None;
                live.contains(tag)
            });
            runtime.skins.clear();
            runtime.skin_sets.clear();
            // `sub_1000128A0` does not inspect or reset the event-dispatch
            // byte or the deferred-close list. A closeAll issued from a
            // callback is immediate, while the retained event snapshot keeps
            // dispatching and any already deferred close requests remain for
            // the update epilogue.
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn request_animation_close(runtime: &mut AnimationRuntime, tag: String) -> Option<u64> {
    // `sub_100012A28` checks wrapper byte +0xE9 before it even looks the tag
    // up. Its +0xD8 std::list preserves request order and suppresses duplicate
    // strings with a linear scan (`0x100012A50..0x100012AC0`).
    if runtime.dispatching_events {
        if !runtime
            .deferred_close_tags
            .iter()
            .any(|pending| pending == &tag)
        {
            runtime.deferred_close_tags.push(tag);
        }
        return None;
    }
    if !runtime.definitions.contains_key(&tag) {
        return None;
    }
    if let Some(playback) = runtime.playback.get_mut(&tag) {
        if let Some(control) = playback.current_control_mut() {
            // Concrete close clears the current Control's completion delegate
            // before erasing the wrapper map (0x100012BBC..0x100012BC0).
            control.callback_installed = false;
        }
        playback.wrapper_control_present = false;
        playback.detached_current = None;
    }
    runtime.skins.remove(&tag);
    runtime.skin_sets.remove(&tag);
    runtime.shaders.remove(&tag);
    let generation = if let Some(generation) = runtime.scene_generations.get(&tag) {
        *generation
    } else {
        // Synthetic animation fixtures can install scene tables directly.
        runtime.next_scene_generation += 1;
        runtime
            .scene_generations
            .insert(tag, runtime.next_scene_generation);
        runtime.next_scene_generation
    };
    Some(generation)
}

pub(super) fn finish_animation_close(lua: &Lua, generation: u64) -> LuaResult<()> {
    // The deletion is a zero-delay event, inserted after already-pending Lua
    // completions. Both direct and deferred closes share these call sites.
    post_animation_entity_removal(lua, generation)?;
    dispatch_registered_application_events(lua)?;
    dispatch_registered_application_events(lua)?;
    Ok(())
}

fn remove_animation_scene(
    runtime: &mut AnimationRuntime,
    callbacks: &Table,
    tag: &str,
) -> LuaResult<()> {
    if let Some(playback) = runtime.playback.get_mut(tag) {
        // The wrapper can still own a current control after root removal.
        // Its old component no longer advances, but pause/resume/isPlaying
        // continue to address that retained object until the map is erased.
        let retained = playback.current_control().cloned();
        playback.controls.clear();
        playback.latched_targets.clear();
        playback.detached_current = retained;
        if !playback.wrapper_control_present {
            runtime.playback.remove(tag);
        }
    }
    runtime.scene_generations.remove(tag);
    runtime.pending_events.remove(tag);
    runtime.pending_event_tags.retain(|pending| pending != tag);
    runtime.actions.remove(tag);
    runtime.definitions.remove(tag);
    runtime.sprite_geometry.remove(tag);
    runtime.sprite_metrics.remove(tag);
    runtime.sprite_regions.remove(tag);
    runtime.transforms.remove(tag);
    runtime.matrices.remove(tag);
    runtime.descendant_reflections.remove(tag);
    runtime.shaders.remove(tag);
    callbacks.raw_set(tag, Value::Nil)?;
    Ok(())
}

fn animation_resource_snapshot(
    resources: &ResourceRuntime,
    definition: &AnimationDefinition,
    data_root: &Path,
) -> AnimationResourceSnapshot {
    let mut names = BTreeSet::new();
    for action in definition.actions.values() {
        for target in action.targets.values() {
            if target.sprite_kind == AnimationSpriteTrackKind::DirectSprite {
                for (_, sprite) in &target.sprite {
                    if !sprite.is_empty() {
                        names.insert(sprite.rsplit('/').next().unwrap_or(sprite).to_owned());
                    }
                }
            }
        }
    }
    let regions = names
        .into_iter()
        .filter_map(|name| {
            resources
                .active_atlas_catalog_region(&name, data_root)
                .map(|region| (name, (*region).clone()))
        })
        .collect::<BTreeMap<_, _>>();
    // `sub_100469030` stores one concrete AtlasSprite pointer at +0x188.
    // Derive every retained query value from that same pointer snapshot: a
    // same-named CompoSprite is a different native type and a missing atlas
    // leaves +0x188 null until a later setter call.
    let geometry = regions
        .iter()
        .map(|(name, region)| {
            let sprite = &region.sprite;
            (
                name.clone(),
                SpriteGeometry {
                    min_x: -f64::from(sprite.pivot_x),
                    min_y: -f64::from(sprite.pivot_y),
                    max_x: f64::from(sprite.width) - f64::from(sprite.pivot_x),
                    max_y: f64::from(sprite.height) - f64::from(sprite.pivot_y),
                },
            )
        })
        .collect();
    let metrics = regions
        .iter()
        .map(|(name, region)| {
            let sprite = &region.sprite;
            (
                name.clone(),
                NativeSpriteMetrics {
                    width: i32::from(sprite.width),
                    height: i32::from(sprite.height),
                    pivot_x: i32::from(sprite.pivot_x),
                    pivot_y: i32::from(sprite.pivot_y),
                },
            )
        })
        .collect();
    (geometry, metrics, regions)
}

pub(super) fn install_cache(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "clearCache",
        lua.create_function(move |_, _: MultiValue| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            // sub_1000E251C erases only the bundle/AppData JSON cache maps.
            // Loaded scenes and their active playback survive this call.
            runtime.bundle_cache.clear();
            runtime.app_data_cache.clear();
            runtime.bundle_json_cache.clear();
            runtime.app_data_json_cache.clear();
            Ok(())
        })?,
    )?;
    for (method, from_bundle) in [("preloadFromBundle", true), ("preloadFromAppData", false)] {
        let runtime = Arc::clone(&animation_runtime);
        let animation_root = if from_bundle {
            Arc::clone(&data_root)
        } else {
            Arc::new(app_data_root(&data_root))
        };
        animation_native.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                let filename = native_required_string(&args, 0, method)?;
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                let cache = if from_bundle {
                    &mut runtime.bundle_json_cache
                } else {
                    &mut runtime.app_data_json_cache
                };
                cached_document(cache, &animation_root, &filename)?;
                // sub_10001D22C is the void one-string Lua adapter.
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

fn cached_document(
    cache: &mut BTreeMap<String, serde_json::Value>,
    root: &Path,
    filename: &str,
) -> LuaResult<serde_json::Value> {
    if let Some(document) = cache.get(filename) {
        return Ok(document.clone());
    }
    let bytes = read_animation_bytes(root, filename)?;
    // JSONCache::load performs operator[] after file acquisition but before
    // parsing (0x1000E2C08/0x1000E2C14). A parse error retains a null entry;
    // an I/O error does not. Neither reaches the scene builder.
    cache.insert(filename.to_owned(), serde_json::Value::Null);
    let document = parse_animation_document(&bytes, filename)?;
    cache.insert(filename.to_owned(), document.clone());
    Ok(document)
}
