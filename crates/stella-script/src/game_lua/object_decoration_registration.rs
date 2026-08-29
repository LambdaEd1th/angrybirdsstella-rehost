//! Decoration and pivot members kept at their separate native registration sites.

use crate::*;

fn required_table(value: Value, context: &str) -> LuaResult<mlua::Table> {
    match value {
        Value::Table(table) => Ok(table),
        _ => Err(runtime_error(format!("{context} must be table"))),
    }
}

fn required_string(value: Value, context: &str) -> LuaResult<String> {
    match value {
        Value::String(value) => Ok(value.to_str()?.to_owned()),
        _ => Err(runtime_error(format!("{context} must be string"))),
    }
}

pub(super) fn install_decoration(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setDecorationObjects",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setDecorationObjects")?;
            let world = object_world(lua)?;
            let entry = required_table(
                world.raw_get::<Value>(name.as_str())?,
                "setDecorationObjects objects.world entry",
            )?;
            let definition_name = required_string(
                entry.get::<Value>("definition")?,
                "setDecorationObjects definition",
            )?;
            let environment = game_environment(lua)?;
            let blocks = required_table(
                environment.get::<Value>("blocks")?,
                "setDecorationObjects blocks",
            )?;
            let definition = required_table(
                blocks.raw_get::<Value>(definition_name.as_str())?,
                "setDecorationObjects block definition",
            )?;
            let decorations = required_table(
                definition.get::<Value>("decorations")?,
                "setDecorationObjects decorations",
            )?;
            let objects = required_table(
                decorations.get::<Value>("objects")?,
                "setDecorationObjects decorations.objects",
            )?;
            let amount =
                native_fcvtzs_f32(
                    table_required_number(&objects, "amount", "setDecorationObjects")? as f32,
                );
            let sprite = table_required_string(&objects, "sprite", "setDecorationObjects")?;
            let angle_increment =
                table_required_number(&objects, "angleIncrement", "setDecorationObjects")? as f32;
            let scale = table_required_number(&objects, "scale", "setDecorationObjects")? as f32;

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.game_lua_object_mut(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            object.decoration = Some(Arc::new(ObjectDecoration {
                amount,
                sprite,
                angle_increment: f64::from(angle_increment),
                scale: f64::from(scale),
            }));
            Ok(())
        })?,
    )
}

pub(super) fn install_pivot(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setPivotOffset",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setPivotOffset")?;
            let offset_x = native_required_number(&args, 1, "setPivotOffset")? as f32;
            let offset_y = native_required_number(&args, 2, "setPivotOffset")? as f32;

            // sub_100040228 writes Lua fields before its throwing native
            // lookup, so a Lua-only entry retains both writes on error.
            let world = object_world(lua)?;
            let entry = required_table(
                world.raw_get::<Value>(name.as_str())?,
                "setPivotOffset objects.world entry",
            )?;
            entry.set("pivotOffsetX", f64::from(offset_x))?;
            entry.set("pivotOffsetY", f64::from(offset_y))?;

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.game_lua_object_mut(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            object.pivot_offset_x = f64::from(offset_x);
            object.pivot_offset_y = f64::from(offset_y);
            Ok(())
        })?,
    )
}
