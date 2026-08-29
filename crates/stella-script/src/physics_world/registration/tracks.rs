//! PhysicsWorld track construction Lua binding.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

fn native_lua51_truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let track_bridge = Arc::clone(&render);
    globals.set(
        "createTrack",
        lua.create_function(move |_, args: MultiValue| {
            // sub_10003CD0C uses positive stack index 1 rather than -1.
            let descriptor = native_required_table(&args, 0, "createTrack")?;
            let points_table = match descriptor.get::<Value>("points")? {
                Value::Table(table) => table,
                _ => return Err(runtime_error("createTrack points must be table")),
            };
            let blocks_table = match descriptor.get::<Value>("blocks")? {
                Value::Table(table) => table,
                _ => return Err(runtime_error("createTrack blocks must be table")),
            };
            let mut points = Vec::with_capacity(native_lua51_table_entry_count(&points_table)?);
            let mut point_index = 1;
            while point_index <= native_lua51_table_entry_count(&points_table)? {
                let Value::Table(point) = points_table.get::<Value>(point_index)? else {
                    return Err(runtime_error(format!(
                        "createTrack point #{point_index} must be table"
                    )));
                };
                let x = point.get::<Value>("x")?;
                let y = point.get::<Value>("y")?;
                points.push((
                    native_lua51_number(&x).unwrap_or(0.0),
                    native_lua51_number(&y).unwrap_or(0.0),
                ));
                point_index += 1;
            }
            if points.is_empty() {
                return Ok(());
            }
            let mut index = 1;
            while index <= native_lua51_table_entry_count(&blocks_table)? {
                let value = blocks_table.get::<Value>(index)?;
                // sub_100529FB4 delegates to lua_tolstring. Numbers are
                // formatted by Purple's float VM; other types yield a null
                // pointer and therefore an empty std::string.
                let object = native_lua51_string(&value).unwrap_or_default();
                {
                    let bridge = track_bridge.lock().expect("render bridge lock poisoned");
                    if !bridge.game_lua_object_exists(&object) {
                        return Err(runtime_error(format!("Missing object: {object}")));
                    }
                }

                // sub_10003CD0C performs the throwing object lookup first and
                // then reads both flags for every block. LuaObject::operator
                // bool delegates to lua_toboolean, so every value except nil
                // and literal false is true.
                let open_ended = native_lua51_truthy(&descriptor.get::<Value>("openEnded")?);
                let rotate_block = native_lua51_truthy(&descriptor.get::<Value>("rotateBlock")?);

                let mut bridge = track_bridge.lock().expect("render bridge lock poisoned");
                if !bridge.game_lua_object_exists(&object) {
                    return Err(runtime_error(format!("Missing object: {object}")));
                }
                bridge.tracks.insert(
                    object.clone(),
                    PhysicsTrack {
                        object,
                        points: points.clone(),
                        _open_ended: open_ended,
                        rotate_block,
                        current_segment: -1,
                        edge_start: (0.0, 0.0),
                        edge_end: (0.0, 0.0),
                        angle: 0.0,
                        impulse_x: 0.0,
                        impulse_y: 0.0,
                    },
                );
                index += 1;
            }
            Ok(())
        })?,
    )?;

    Ok(())
}
