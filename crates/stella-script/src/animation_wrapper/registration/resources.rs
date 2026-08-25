//! Animation scene loading, closing and parsed-asset cache bindings.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

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
            lua.create_function(move |_, args: MultiValue| {
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
                    let asset =
                        cached.unwrap_or_else(|| animation_asset(&animation_root, &filename));
                    if from_bundle {
                        runtime
                            .bundle_cache
                            .entry(filename.clone())
                            .or_insert_with(|| asset.clone());
                    } else {
                        runtime
                            .app_data_cache
                            .entry(filename.clone())
                            .or_insert_with(|| asset.clone());
                    }
                    asset
                };
                if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                    eprintln!(
                        "animation-native load tag={tag} file={filename} actions={:?}",
                        asset.actions
                    );
                }
                let (geometry, metrics, regions) = animation_resource_snapshot(
                    &resources.lock().expect("resource runtime lock poisoned"),
                    &asset.definition,
                    &sprite_data_root,
                );
                install_animation_asset(
                    &mut runtime.lock().expect("animation runtime lock poisoned"),
                    tag,
                    asset,
                    geometry,
                    metrics,
                    regions,
                );
                // sub_10001D43C is the void two-string Lua adapter.
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

pub(super) fn install_closing(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "close",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "close")?;
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            runtime.playback.remove(&tag);
            runtime.pending_events.remove(&tag);
            runtime.pending_event_tags.retain(|pending| pending != &tag);
            runtime.actions.remove(&tag);
            runtime.definitions.remove(&tag);
            runtime.sprite_geometry.remove(&tag);
            runtime.sprite_metrics.remove(&tag);
            runtime.sprite_regions.remove(&tag);
            runtime.transforms.remove(&tag);
            runtime.matrices.remove(&tag);
            runtime.descendant_reflections.remove(&tag);
            runtime.skins.remove(&tag);
            runtime.shaders.remove(&tag);
            callbacks.raw_set(tag, Value::Nil)?;
            Ok(())
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "closeAll",
        lua.create_function(move |_, _: MultiValue| {
            let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
            runtime.playback.clear();
            runtime.pending_events.clear();
            runtime.pending_event_tags.clear();
            runtime.actions.clear();
            runtime.definitions.clear();
            runtime.sprite_geometry.clear();
            runtime.sprite_metrics.clear();
            runtime.sprite_regions.clear();
            runtime.transforms.clear();
            runtime.matrices.clear();
            runtime.descendant_reflections.clear();
            runtime.skins.clear();
            runtime.shaders.clear();
            callbacks.clear()?;
            Ok(())
        })?,
    )?;
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
                .map(|region| (name, region))
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
                    &mut runtime.bundle_cache
                } else {
                    &mut runtime.app_data_cache
                };
                cache
                    .entry(filename.clone())
                    .or_insert_with(|| animation_asset(&animation_root, &filename));
                // sub_10001D22C is the void one-string Lua adapter.
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
