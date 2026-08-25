//! `drawPolygon` / `sub_100043F28`, registered at `0x10002DA48`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawPolygon",
        lua.create_function(move |_, args: MultiValue| {
            // Direct member sub_100043F28 requires an exact TABLE in slot
            // one and exact NUMBER slots two through seven, while ignoring
            // any additional Lua arguments.
            let vertices = native_required_table(&args, 0, "drawPolygon")?;
            let offset_x = native_required_number(&args, 1, "drawPolygon")?;
            let offset_y = native_required_number(&args, 2, "drawPolygon")?;
            let red = native_required_number(&args, 3, "drawPolygon")?;
            let green = native_required_number(&args, 4, "drawPolygon")?;
            let blue = native_required_number(&args, 5, "drawPolygon")?;
            let alpha = native_required_number(&args, 6, "drawPolygon")?;

            // LuaTable::size at sub_10052B324 counts every key with
            // lua_next. The member then indexes 1..=count with raw integer
            // lookup and requires each resulting entry to be a table.
            let mut entry_count = 0_usize;
            for pair in vertices.clone().pairs::<Value, Value>() {
                pair?;
                entry_count += 1;
            }
            let mut points = Vec::with_capacity(entry_count);
            for index in 1..=entry_count {
                let point = match vertices.raw_get::<Value>(index)? {
                    Value::Table(point) => point,
                    _ => {
                        return Err(runtime_error(format!(
                            "drawPolygon vertex at index {index} must be a table"
                        )));
                    }
                };
                // Field access uses ordinary lua_gettable, then lua_tonumber:
                // numeric strings convert while every other value becomes 0.
                let x = native_lua51_number(&point.get::<Value>("x")?).unwrap_or(0.0);
                let y = native_lua51_number(&point.get::<Value>("y")?).unwrap_or(0.0);
                points.push((f64::from(x as f32), f64::from(y as f32)));
            }
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let state = bridge.state;
            for command in native_polygon_commands(
                &points,
                offset_x,
                offset_y,
                [red, green, blue, alpha],
                state,
            ) {
                bridge.push_rect_command(command);
            }
            Ok(())
        })?,
    )
}
