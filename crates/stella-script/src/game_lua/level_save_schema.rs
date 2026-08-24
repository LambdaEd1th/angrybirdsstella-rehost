//! Field whitelist and sensor branches from `GameLua::saveLevel`.

use super::level_table_clone::*;
use crate::*;

/// Build the filtered level table produced by Purple's `saveLevel`
/// (`sub_10004730C`). This path is deliberately distinct from the general
/// AppData serializer: the editor exports only a fixed level schema and only
/// admits per-object extensions named by `editableAttributes`.
pub(super) fn native_saved_level_table(lua: &Lua, objects: &mlua::Table) -> LuaResult<mlua::Table> {
    const ROOT_FIELDS: [(&str, NativeLevelFieldKind); 15] = [
        ("trainCarts", NativeLevelFieldKind::Table),
        ("theme", NativeLevelFieldKind::String),
        ("birdCameraData", NativeLevelFieldKind::Table),
        ("castleCameraData", NativeLevelFieldKind::Table),
        ("focusCameraData", NativeLevelFieldKind::Table),
        ("physicsToWorld", NativeLevelFieldKind::Number),
        ("joints", NativeLevelFieldKind::Table),
        ("tracks", NativeLevelFieldKind::Table),
        ("counts", NativeLevelFieldKind::Table),
        ("doNotWaitForMovingObjects", NativeLevelFieldKind::Boolean),
        ("themeSprites", NativeLevelFieldKind::Table),
        ("gravityForceMultiplier", NativeLevelFieldKind::Number),
        ("waterForceMultiplier", NativeLevelFieldKind::Number),
        ("worldGravity", NativeLevelFieldKind::Number),
        ("variantGroups", NativeLevelFieldKind::Table),
    ];

    let output = lua.create_table()?;
    for (field, kind) in ROOT_FIELDS {
        native_copy_level_field(lua, objects, &output, field, kind, None)?;
    }
    native_copy_level_field(
        lua,
        objects,
        &output,
        "variantProbabilities",
        NativeLevelFieldKind::Table,
        None,
    )?;

    let saved_world = lua.create_table()?;
    if let Value::Table(world) = objects.raw_get::<Value>("world")? {
        for pair in world.pairs::<Value, Value>() {
            let (Value::String(world_key), Value::Table(source)) = pair? else {
                continue;
            };
            let block_name = world_key.to_string_lossy();
            let definition =
                native_lua51_string(&source.raw_get::<Value>("definition")?).unwrap_or_default();
            if matches!(
                definition.as_str(),
                "BLOCK_SENSOR_PIG_A" | "BLOCK_SENSOR_PIG_B"
            ) {
                continue;
            }

            let block = lua.create_table()?;
            for field in ["angle", "x", "y"] {
                native_set_required_level_number(&source, &block, field)?;
            }
            native_set_required_level_string(&source, &block, "name")?;
            block.raw_set("definition", definition)?;
            native_set_required_level_number(&source, &block, "z_order")?;

            native_copy_level_field(
                lua,
                &source,
                &block,
                "active",
                NativeLevelFieldKind::Boolean,
                None,
            )?;
            if let Some(category) =
                native_lua51_string(&source.raw_get::<Value>("gravityFilterCategory")?)
                && category != "NONE"
            {
                block.raw_set("gravityFilterCategory", category)?;
            }
            native_copy_level_field(
                lua,
                &source,
                &block,
                "triggerEvents",
                NativeLevelFieldKind::Table,
                None,
            )?;
            for field in ["scale", "scaleX", "scaleY"] {
                native_copy_level_field(
                    lua,
                    &source,
                    &block,
                    field,
                    NativeLevelFieldKind::Number,
                    None,
                )?;
            }
            native_copy_level_field(
                lua,
                &source,
                &block,
                "horFlip",
                NativeLevelFieldKind::Boolean,
                None,
            )?;
            native_copy_level_field(
                lua,
                &source,
                &block,
                "themeTexture",
                NativeLevelFieldKind::String,
                None,
            )?;
            for field in [
                "startNumber",
                "startNumberDecimal",
                "episodeType",
                "pageNumber",
                "shotPattern",
                "levelNumber",
            ] {
                native_copy_level_field(
                    lua,
                    &source,
                    &block,
                    field,
                    NativeLevelFieldKind::Number,
                    None,
                )?;
            }
            native_copy_level_field(
                lua,
                &source,
                &block,
                "area",
                NativeLevelFieldKind::String,
                None,
            )?;
            for field in ["groupingIndex", "groupVariantIndex"] {
                native_copy_level_field(
                    lua,
                    &source,
                    &block,
                    field,
                    NativeLevelFieldKind::Number,
                    None,
                )?;
            }

            let sensor_type = native_lua51_string(&source.raw_get::<Value>("sensorType")?);
            match sensor_type.as_deref() {
                Some("gravitation") => {
                    native_set_required_level_number(&source, &block, "gravitationMinForce")?;
                    native_set_required_level_number(&source, &block, "gravitationMaxForce")?;
                    if native_lua51_truthy(&source.raw_get::<Value>("isWater")?) {
                        native_set_required_level_number(&source, &block, "waterDensityZeroLevel")?;
                    }
                    if native_lua51_number(&source.raw_get::<Value>("radius")?).is_some() {
                        native_set_required_level_number(&source, &block, "radius")?;
                    } else {
                        native_set_required_level_number(&source, &block, "width")?;
                        native_set_required_level_number(&source, &block, "height")?;
                        native_set_required_level_number(&source, &block, "forceAngle")?;
                    }
                }
                Some("stream") => {
                    native_set_required_level_number(&source, &block, "radius")?;
                    native_set_required_level_number(&source, &block, "force")?;
                    for field in ["nodes", "vertices"] {
                        native_copy_required_level_table(lua, &source, &block, field)?;
                    }
                }
                Some("killing") => {
                    native_set_required_level_number(&source, &block, "width")?;
                    native_set_required_level_number(&source, &block, "height")?;
                }
                Some("collectible") | Some(_) | None => {}
            }

            if native_lua51_truthy(&source.raw_get::<Value>("canBeEdited")?) {
                for field in [
                    "explosionRadius",
                    "explosionForce",
                    "explosionDamageRadius",
                    "explosionDamage",
                    "startingForce",
                    "forceAngle",
                    "suckerTransmitSpeed",
                    "suckerExitSpeed",
                ] {
                    native_copy_level_field(
                        lua,
                        &source,
                        &block,
                        field,
                        NativeLevelFieldKind::Number,
                        None,
                    )?;
                }
            }

            if let Value::Table(attributes) = source.raw_get::<Value>("editableAttributes")? {
                for index in 1..=attributes.raw_len() {
                    let Some(attribute) = native_lua51_string(&attributes.raw_get::<Value>(index)?)
                    else {
                        continue;
                    };
                    let value = source.raw_get::<Value>(attribute.as_str())?;
                    if matches!(value, Value::Nil) {
                        continue;
                    }
                    let context = Some((attribute.as_str(), block_name.as_ref()));
                    let value = native_level_editable_value(lua, &value, context)?;
                    block.raw_set(attribute.as_str(), value)?;
                }
            }

            saved_world.raw_set(world_key, block)?;
        }
    }
    output.raw_set("world", saved_world)?;
    Ok(output)
}
