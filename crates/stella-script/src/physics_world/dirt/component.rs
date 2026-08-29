//! Lua definition lookup and host-side DirtComponent construction mirror.

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Value};

use crate::*;

fn dirt_textures_for_object(lua: &Lua, object_name: &str) -> LuaResult<Option<(String, String)>> {
    let world = object_world(lua)?;
    let Value::Table(entry) = world.raw_get::<Value>(object_name)? else {
        return Ok(None);
    };
    let definition_name = entry.get::<String>("definition").unwrap_or_default();
    if definition_name.is_empty() {
        return Ok(None);
    }
    let environment = game_environment(lua)?;
    let definition = match environment.get::<Value>("blocks")? {
        Value::Table(blocks) => match blocks.raw_get::<Value>(definition_name.as_str())? {
            Value::Table(definition) => Some(definition),
            _ => None,
        },
        _ => None,
    }
    .or_else(|| {
        let Value::Table(block_table) = environment.get::<Value>("blockTable").ok()? else {
            return None;
        };
        let Value::Table(blocks) = block_table.get::<Value>("blocks").ok()? else {
            return None;
        };
        match blocks.raw_get::<Value>(definition_name.as_str()).ok()? {
            Value::Table(definition) => Some(definition),
            _ => None,
        }
    });
    let Some(definition) = definition else {
        return Ok(None);
    };
    let Value::Table(components) = definition.get::<Value>("components")? else {
        return Ok(None);
    };
    let Value::Table(dirt) = components.get::<Value>("dirt")? else {
        return Ok(None);
    };
    let background = dirt.get::<String>("bgTexture").unwrap_or_default();
    let foreground = dirt.get::<String>("fgTexture").unwrap_or_default();
    Ok((!background.is_empty() && !foreground.is_empty()).then_some((background, foreground)))
}

pub(crate) fn ensure_dirt_component(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    resources: &Arc<Mutex<ResourceRuntime>>,
    data_root: &Path,
    object_name: &str,
) -> LuaResult<bool> {
    let Some((background, foreground)) = dirt_textures_for_object(lua, object_name)? else {
        return Ok(false);
    };
    // sub_10001F98C snapshots a b2FixtureDef from
    // blockTable.materials[objects.world[name].material]. Later cuts only
    // replace its shape pointer, so live fixture setters do not alter it.
    let material_properties = (|| -> LuaResult<Option<(f64, f64, f64)>> {
        let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(object_name)? else {
            return Ok(None);
        };
        let material = entry.get::<String>("material").unwrap_or_default();
        if material.is_empty() {
            return Ok(None);
        }
        let environment = game_environment(lua)?;
        let Value::Table(block_table) = environment.get::<Value>("blockTable")? else {
            return Ok(None);
        };
        let Value::Table(materials) = block_table.get::<Value>("materials")? else {
            return Ok(None);
        };
        let Value::Table(properties) = materials.raw_get::<Value>(material)? else {
            return Ok(None);
        };
        Ok(Some((
            f64::from(properties.get::<f64>("density").unwrap_or_default() as f32),
            f64::from(properties.get::<f64>("friction").unwrap_or(0.2) as f32),
            f64::from(properties.get::<f64>("restitution").unwrap_or_default() as f32),
        )))
    })()?;
    // sub_10001F98C resolves both AtlasSprite names through
    // sub_10045BC64/sub_10046B1F0 and snapshots the returned image pointers
    // before constructing either DrawablePolygon. Do this before taking the
    // render lock so the deferred host retains the same lifetime boundary.
    let (background_binding, foreground_binding) = {
        let resources = resources.lock().expect("resource runtime lock poisoned");
        let resolve = |name: &str| {
            resources
                .active_atlas_catalog_region(name, data_root)
                .map(|region| MaskedTextureBinding::Source(region.texture_source.clone()))
                .unwrap_or(MaskedTextureBinding::Missing)
        };
        (resolve(&background), resolve(&foreground))
    };
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let Some(object) = bridge.game_lua_object_mut(object_name) else {
        return Ok(false);
    };
    if object.dirt.is_none()
        && let Some(mut dirt) = DirtComponent::from_object(
            object,
            DirtTextures {
                background: background.clone(),
                foreground: foreground.clone(),
                background_binding,
                foreground_binding,
            },
            material_properties.map_or(object.density, |properties| properties.0),
            material_properties.map_or(object.friction, |properties| properties.1),
            material_properties.map_or(object.restitution, |properties| properties.2),
        )
    {
        // Some level paths publish `definition` after constructing the
        // component. Preserve cuts queued in that gap.
        for hole in object.dirt_holes.iter().copied() {
            dirt.cut(hole);
        }
        object.dirt = Some(Arc::new(dirt));
        if std::env::var_os("STELLA_TRACE_DIRT").is_some() {
            eprintln!(
                "dirt component {object_name}: background={background:?} foreground={foreground:?}"
            );
        }
    }
    Ok(object.dirt.is_some())
}
