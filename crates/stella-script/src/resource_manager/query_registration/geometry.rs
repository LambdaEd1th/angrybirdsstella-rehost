//! Sprite/composite query members at `sub_100448EB4` and successors.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let bounds_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "getSpriteBounds",
        lua.create_function(move |_, args: MultiValue| {
            let index = usize::from(args.len() != 1);
            let name = native_required_borrowed_string(&args, index, "getSpriteBounds")?;
            Ok(bounds_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .active_native_sprite_metrics(name)
                .map(|metrics| (f64::from(metrics.width), f64::from(metrics.height)))
                .unwrap_or((0.0, 0.0)))
        })?,
    )?;

    let pivot_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "getSpritePivot",
        lua.create_function(move |_, args: MultiValue| {
            let index = usize::from(args.len() != 1);
            let name = native_required_borrowed_string(&args, index, "getSpritePivot")?;
            Ok(pivot_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .active_native_sprite_metrics(name)
                .map(|metrics| (f64::from(metrics.pivot_x), f64::from(metrics.pivot_y)))
                .unwrap_or((0.0, 0.0)))
        })?,
    )?;

    let composite_bounds_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "getCompoSpriteBounds",
        lua.create_function(move |_, args: MultiValue| {
            let index = usize::from(args.len() != 1);
            let name = args
                .iter()
                .nth(index)
                .and_then(value_string)
                .ok_or_else(|| {
                    runtime_error(format!(
                        "getCompoSpriteBounds argument {} must be string",
                        index + 1
                    ))
                })?;
            let mut resources = composite_bounds_resources
                .lock()
                .expect("resource runtime lock poisoned");
            let Some(metrics) = resources.refresh_active_composite_metrics(&name) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Number(-f64::from(metrics.pivot_x)),
                Value::Number(-f64::from(metrics.pivot_y)),
                Value::Number(f64::from(metrics.width) - f64::from(metrics.pivot_x)),
                Value::Number(f64::from(metrics.height) - f64::from(metrics.pivot_y)),
            ]))
        })?,
    )?;

    let data_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "getCompoSpriteData",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "getCompoSpriteData")?;
            let result = lua.create_table()?;
            let resources = data_resources
                .lock()
                .expect("resource runtime lock poisoned");
            // sub_1004493BC dereferences the native CompoSprite immediately;
            // unlike the entry queries it has no missing-resource branch.
            // Surface the invalid lookup as a recoverable Lua error instead
            // of manufacturing the empty table previously returned here.
            let parts = resources.active_composite_parts(&name).ok_or_else(|| {
                runtime_error(format!(
                    "getCompoSpriteData composite resource '{name}' was not found"
                ))
            })?;
            for (index, part) in parts.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.raw_set(1, part.sprite.as_str())?;
                entry.raw_set(2, part.x)?;
                entry.raw_set(3, part.y)?;
                result.raw_set(index + 1, entry)?;
            }
            Ok(result)
        })?,
    )?;

    let entry_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "getCompoSpriteEntry",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "getCompoSpriteEntry")?;
            let resources = entry_resources
                .lock()
                .expect("resource runtime lock poisoned");
            let Some(parts) = resources.active_composite_parts(&name) else {
                // sub_1004497D8 logs this case and returns zero results. A
                // single nil is observably different through Lua's select.
                return Ok(MultiValue::new());
            };
            let Some(selector) =
                native_composite_selector(args.iter().nth(1), "getCompoSpriteEntry")?
            else {
                return Ok(MultiValue::new());
            };
            let index = composite_part_index(parts, &selector).ok_or_else(|| {
                // The original raw vector/map access has no safe failure
                // path and subsequently dereferences the invalid pointer.
                runtime_error("getCompoSpriteEntry selector did not resolve to a composite part")
            })?;
            let table = composite_part_lua_table(lua, &parts[index])?;
            Ok(MultiValue::from_vec(vec![Value::Table(table)]))
        })?,
    )?;

    resource_api.set(
        "setCompoSpriteEntry",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setCompoSpriteEntry")?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            // Native checks the resource before inspecting either the selector
            // or the value table.
            let Some(parts) = resources.active_composite_parts(&name) else {
                return Ok(());
            };
            let Some(selector) =
                native_composite_selector(args.iter().nth(1), "setCompoSpriteEntry")?
            else {
                return Ok(());
            };
            let index = composite_part_index(parts, &selector).ok_or_else(|| {
                runtime_error("setCompoSpriteEntry selector did not resolve to a composite part")
            })?;
            let values = native_required_table(&args, 2, "setCompoSpriteEntry")?;
            let (old_sprite, new_sprite, updated) = {
                let parts = resources
                    .active_composite_parts_mut(&name)
                    .expect("composite existence was checked above");
                let old_sprite = parts[index].sprite.clone();
                update_composite_part_from_lua(&mut parts[index], &values)?;
                (old_sprite, parts[index].sprite.clone(), parts.clone())
            };
            if old_sprite != new_sprite
                && resources
                    .rebind_active_composite_part_region(&name, index, &new_sprite)
                    .is_none()
            {
                return Err(runtime_error(format!(
                    "setCompoSpriteEntry atlas resource '{new_sprite}' was not found"
                )));
            }
            resources
                .refresh_active_bound_composite(&name)
                .expect("active composite must retain its concrete owner");
            resources.mark_sprite_catalog_changed();
            drop(resources);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.composite_updates.insert(name, updated);
            Ok(())
        })?,
    )?;
    Ok(())
}

#[derive(Debug)]
enum NativeCompositeSelector {
    Index(u32),
    Name(String),
}

/// Reproduce the handwritten number-then-string dispatch in
/// sub_1004497D8/sub_100449CFC. Lua 5.1 reports numeric strings through
/// `lua_isnumber`, after which Purple's strict `getFloat` rejects their
/// actual STRING tag; they must not fall through to the name lookup.
fn native_composite_selector(
    value: Option<&Value>,
    function: &str,
) -> LuaResult<Option<NativeCompositeSelector>> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::Integer(index) => Ok(Some(NativeCompositeSelector::Index(native_fcvtzs_f32(
            *index as f32,
        ) as u32))),
        Value::Number(index) => Ok(Some(NativeCompositeSelector::Index(native_fcvtzs_f32(
            *index as f32,
        ) as u32))),
        Value::String(_) if native_lua51_number(value).is_some() => Err(runtime_error(format!(
            "bad argument #2 to '{function}' (number expected)"
        ))),
        Value::String(name) => Ok(Some(NativeCompositeSelector::Name(name.to_string_lossy()))),
        _ => Ok(None),
    }
}

fn composite_part_index(
    parts: &[stella_assets::ka3d::CompositePart],
    selector: &NativeCompositeSelector,
) -> Option<usize> {
    match selector {
        NativeCompositeSelector::Index(index) => {
            let index = *index as usize;
            (index < parts.len()).then_some(index)
        }
        NativeCompositeSelector::Name(name) => parts.iter().position(|part| part.sprite == *name),
    }
}
