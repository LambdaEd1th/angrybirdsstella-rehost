//! Composite-part Lua records and mutation semantics.

use mlua::{Lua, Result as LuaResult, Value};
use stella_assets::ka3d::CompositePart;

use crate::native_lua51_number;

pub(crate) fn composite_part_lua_table(lua: &Lua, part: &CompositePart) -> LuaResult<mlua::Table> {
    let table = lua.create_table()?;
    table.set("name", part.sprite.as_str())?;
    table.set("x", part.x)?;
    table.set("y", part.y)?;
    table.set("scaleX", part.scale_x)?;
    table.set("scaleY", part.scale_y)?;
    table.set("flipX", part.flip_x < 0.0)?;
    table.set("flipY", part.flip_y < 0.0)?;
    table.set("angle", part.angle)?;
    table.set("visible", part.visible)?;
    Ok(table)
}

pub(crate) fn update_composite_part_from_lua(
    part: &mut CompositePart,
    values: &mlua::Table,
) -> LuaResult<()> {
    let name = values.get::<Value>("name")?;
    if !matches!(name, Value::Nil) {
        // LuaState::getString accepts a native number through Lua 5.1's
        // number-to-string conversion, while other non-string values become
        // the empty resource name. The AtlasSprite lookup itself strips a
        // runtime `#suffix`, but the composite record retains the full name.
        part.sprite = match name {
            Value::String(name) => name.to_string_lossy(),
            Value::Integer(number) => (number as f32).to_string(),
            Value::Number(number) => (number as f32).to_string(),
            _ => String::new(),
        };
    }
    for (field, target) in [
        ("x", &mut part.x),
        ("y", &mut part.y),
        ("scaleX", &mut part.scale_x),
        ("scaleY", &mut part.scale_y),
        ("angle", &mut part.angle),
    ] {
        let value = values.get::<Value>(field)?;
        if !matches!(value, Value::Nil) {
            // sub_10052A014 mirrors lua_tonumber: numeric strings convert,
            // and any other present non-number writes zero rather than
            // preserving the prior field.
            *target = native_lua51_number(&value).unwrap_or(0.0) as f32;
        }
    }
    let flip_x = values.get::<Value>("flipX")?;
    if !matches!(flip_x, Value::Nil) {
        let flip = !matches!(flip_x, Value::Boolean(false) | Value::Nil);
        part.flip_x = if flip { -1.0 } else { 1.0 };
    }
    let flip_y = values.get::<Value>("flipY")?;
    if !matches!(flip_y, Value::Nil) {
        let flip = !matches!(flip_y, Value::Boolean(false) | Value::Nil);
        part.flip_y = if flip { -1.0 } else { 1.0 };
    }
    let visible = values.get::<Value>("visible")?;
    if !matches!(visible, Value::Nil) {
        part.visible = !matches!(visible, Value::Boolean(false) | Value::Nil);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_table_uses_native_float_sign_and_direct_radian_storage() {
        let lua = Lua::new();
        let mut part = CompositePart {
            sprite: "PART#front".to_owned(),
            x: 1.0,
            y: 2.0,
            scale_x: 3.0,
            scale_y: 4.0,
            flip_x: -0.25,
            flip_y: f32::NAN,
            angle: std::f32::consts::FRAC_PI_2,
            visible: true,
        };
        let table = composite_part_lua_table(&lua, &part).unwrap();
        assert!(table.get::<bool>("flipX").unwrap());
        assert!(!table.get::<bool>("flipY").unwrap());
        assert_eq!(
            table.get::<f32>("angle").unwrap().to_bits(),
            std::f32::consts::FRAC_PI_2.to_bits()
        );

        let update = lua.create_table().unwrap();
        update.set("flipX", false).unwrap();
        update.set("flipY", true).unwrap();
        update.set("angle", 0.75).unwrap();
        update_composite_part_from_lua(&mut part, &update).unwrap();
        assert_eq!((part.flip_x, part.flip_y), (1.0, -1.0));
        assert_eq!(part.angle, 0.75);
    }
}
