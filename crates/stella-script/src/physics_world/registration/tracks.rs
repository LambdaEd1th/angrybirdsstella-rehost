//! PhysicsWorld track construction Lua binding.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

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
            let mut points = Vec::with_capacity(points_table.raw_len());
            for index in 1..=points_table.raw_len() {
                let Value::Table(point) = points_table.raw_get::<Value>(index)? else {
                    return Err(runtime_error(format!(
                        "createTrack point #{index} must be table"
                    )));
                };
                points.push((
                    f64::from(table_required_number(&point, "x", "createTrack")? as f32),
                    f64::from(table_required_number(&point, "y", "createTrack")? as f32),
                ));
            }
            if points.is_empty() {
                return Ok(());
            }
            let blocks_table = match descriptor.get::<Value>("blocks")? {
                Value::Table(table) => table,
                _ => return Err(runtime_error("createTrack blocks must be table")),
            };
            let mut blocks = Vec::with_capacity(blocks_table.raw_len());
            for index in 1..=blocks_table.raw_len() {
                let value = blocks_table.raw_get::<Value>(index)?;
                let Some(name) = value_string(&value) else {
                    return Err(runtime_error(format!(
                        "createTrack block #{index} must be string"
                    )));
                };
                blocks.push(name);
            }
            let open_ended = descriptor.get::<bool>("openEnded").unwrap_or(false);
            let rotate_block = descriptor.get::<bool>("rotateBlock").unwrap_or(false);
            let mut bridge = track_bridge.lock().expect("render bridge lock poisoned");
            for object in blocks {
                if !bridge.scene.contains_key(&object) {
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
            }
            Ok(())
        })?,
    )?;

    Ok(())
}
