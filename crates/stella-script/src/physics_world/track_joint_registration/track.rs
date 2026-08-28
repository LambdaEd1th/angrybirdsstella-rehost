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
            let points = match track.raw_get::<Value>("points")? {
                Value::Table(points) => {
                    let mut parsed = Vec::with_capacity(points.raw_len());
                    for index in 1..=points.raw_len() {
                        let Value::Table(point) = points.raw_get::<Value>(index)? else {
                            return Err(runtime_error(format!(
                                "objectAndTrackOverlap point #{index} must be table"
                            )));
                        };
                        parsed.push((
                            f64::from(
                                table_required_number(&point, "x", "objectAndTrackOverlap")? as f32
                            ),
                            f64::from(
                                table_required_number(&point, "y", "objectAndTrackOverlap")? as f32
                            ),
                        ));
                    }
                    Some(parsed)
                }
                _ => None,
            };
            Ok(points.is_some_and(|points| {
                points
                    .windows(2)
                    .any(|pair| object.head_fixture_overlaps_track_segment((pair[0], pair[1])))
            }))
        })?,
    )?;
    Ok(())
}
