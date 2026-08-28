//! Track destruction, current-angle and chain-overlap bindings.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let destroy_track_bridge = Arc::clone(&render);
    globals.set(
        "destroyTrack",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "destroyTrack")?;
            let mut bridge = destroy_track_bridge
                .lock()
                .expect("render bridge lock poisoned");
            if !bridge.scene.contains_key(&name) {
                return Err(runtime_error(format!("Missing object: {name}")));
            }
            // b2Body::DestroyTrack reaches sub_10086E3E0 only after the
            // throwing RenderObjectData lookup. Like every Box2D world
            // mutation, it is a silent no-op while e_locked is set.
            if bridge.physics_world_locked {
                return Ok(());
            }
            if bridge.tracks.remove(&name).is_some() {
                // The native destroy member clears the body's track pointer,
                // wakes it and resets b2Body::m_sleepTime even when it was
                // already awake.
                if let Some(object) = bridge.scene.get_mut(&name) {
                    object.wake();
                }
            }
            Ok(())
        })?,
    )?;
    let track_angle_bridge = Arc::clone(&render);
    globals.set(
        "getCurrentTrackAngle",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "getCurrentTrackAngle")?;
            let bridge = track_angle_bridge
                .lock()
                .expect("render bridge lock poisoned");
            if !bridge.scene.contains_key(&name) {
                return Err(runtime_error(format!("Missing object: {name}")));
            }
            let Some(track) = bridge.tracks.get(&name) else {
                return Ok(0.0);
            };
            let Some(object) = bridge.scene.get(&track.object) else {
                return Ok(0.0);
            };
            Ok(track.native_current_angle((object.x, object.y)))
        })?,
    )?;
    let track_overlap_bridge = Arc::clone(&render);
    globals.set(
        "objectAndTrackOverlap",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "objectAndTrackOverlap")?;
            let track = args.iter().nth(1).and_then(value_table).ok_or_else(|| {
                runtime_error("bad argument #2 to 'objectAndTrackOverlap' (table expected)")
            })?;
            // sub_1000222E4 resolves and type-checks this field before the
            // throwing object lookup. Normal table access intentionally
            // retains a descriptor __index callback.
            let points_table = match track.get::<Value>("points")? {
                Value::Table(points) => points,
                _ => {
                    return Err(runtime_error("objectAndTrackOverlap points must be table"));
                }
            };
            {
                let bridge = track_overlap_bridge
                    .lock()
                    .expect("render bridge lock poisoned");
                let Some(object) = bridge.scene.get(&name) else {
                    // sub_10003D208 uses the same throwing sub_10005DAF8 lookup
                    // as createTrack/getCurrentTrackAngle before it inspects the
                    // temporary chain points.
                    return Err(runtime_error(format!("Missing object: {name}")));
                };
                if !object.has_physics_body() {
                    return Ok(false);
                }
            }
            let mut points = Vec::with_capacity(native_lua51_table_entry_count(&points_table)?);
            let mut index = 1;
            while index <= native_lua51_table_entry_count(&points_table)? {
                let Value::Table(point) = points_table.get::<Value>(index)? else {
                    return Err(runtime_error(format!(
                        "objectAndTrackOverlap point #{index} must be table"
                    )));
                };
                let x = point.get::<Value>("x")?;
                let y = point.get::<Value>("y")?;
                points.push((
                    native_lua51_number(&x).unwrap_or(0.0),
                    native_lua51_number(&y).unwrap_or(0.0),
                ));
                index += 1;
            }
            let bridge = track_overlap_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let Some(object) = bridge.scene.get(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            Ok(points
                .windows(2)
                .any(|pair| object.head_fixture_overlaps_track_segment((pair[0], pair[1]))))
        })?,
    )?;
    Ok(())
}
