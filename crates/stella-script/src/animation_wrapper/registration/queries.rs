//! Animation entity and action query bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

pub(super) fn install_entities(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    let known_animation_entities = Arc::clone(&animation_runtime);
    animation_native.set(
        "containsEntity",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            Ok(known_animation_entities
                .lock()
                .expect("animation runtime lock poisoned")
                .definitions
                .get(&tag)
                .is_some_and(|definition| {
                    animation_definition_contains_entity(definition, &entity)
                }))
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityPosition",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = runtime.lock().expect("animation runtime lock poisoned");
            Ok(animation_entity_local_transform(&runtime, &tag, &entity)
                .map_or((0.0, 0.0), |transform| (transform.x, transform.y)))
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityWorldPosition",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = runtime.lock().expect("animation runtime lock poisoned");
            Ok(animation_entity_world_affine(&runtime, &tag, &entity)
                .map_or((0.0, 0.0), |transform| (transform.x, transform.y)))
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityScale",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = runtime.lock().expect("animation runtime lock poisoned");
            Ok(
                animation_entity_local_transform(&runtime, &tag, &entity).map_or(
                    (1.0, 1.0),
                    |transform| {
                        // sub_100015000 queries the two float matrix-column
                        // magnitudes; it does not expose the authored scalar
                        // fields directly.
                        let affine = AnimationAffine::from_transform(transform);
                        (affine.scale_x(), affine.scale_y())
                    },
                ),
            )
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityWorldScale",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = runtime.lock().expect("animation runtime lock poisoned");
            Ok(animation_entity_world_affine(&runtime, &tag, &entity)
                .map_or((1.0, 1.0), |transform| {
                    (transform.scale_x(), transform.scale_y())
                }))
        })?,
    )?;
    let entity_transform_runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityWorldTransform",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = entity_transform_runtime
                .lock()
                .expect("animation runtime lock poisoned");
            let Some(transform) = animation_entity_world_affine(&runtime, &tag, &entity) else {
                // sub_10000F46C returns zero Lua results when either scene or
                // entity lookup fails.
                return Ok(MultiValue::new());
            };
            let mut values = MultiValue::from_vec(vec![
                Value::Number(transform.x),
                Value::Number(transform.y),
                Value::Number(transform.scale_x()),
                Value::Number(transform.scale_y()),
                Value::Number(transform.angle()),
            ]);
            // The native wrapper only appends this sixth result for entities
            // with a SpriteComponent. It reports whether getSprite() is non-null.
            if let Some(has_sprite) = animation_entity_has_sprite(&runtime, &tag, &entity) {
                values.push_back(Value::Boolean(has_sprite));
            }
            Ok(values)
        })?,
    )?;
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getEntityWorldBounds",
        lua.create_function(move |_, (tag, entity): (String, String)| {
            let runtime = runtime.lock().expect("animation runtime lock poisoned");
            let [left, top, right, bottom] = animation_entity_world_bounds(&runtime, &tag, &entity);
            Ok((left, top, right, bottom))
        })?,
    )?;
    Ok(())
}

pub(super) fn install_actions(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    animation_native.set(
        "getActions",
        lua.create_function(move |lua, tag: String| {
            let table = lua.create_table()?;
            if let Some(actions) = runtime
                .lock()
                .expect("animation runtime lock poisoned")
                .actions
                .get(&tag)
            {
                for (index, action) in actions.keys().enumerate() {
                    table.raw_set(index + 1, action.as_str())?;
                }
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}
