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
            destroy_track_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .tracks
                .remove(&name);
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
            let points = match track.raw_get::<Value>("points")? {
                Value::Table(points) => Some(
                    points
                        .sequence_values::<mlua::Table>()
                        .filter_map(Result::ok)
                        .filter_map(|point| {
                            Some((
                                f64::from(point.get::<f32>("x").ok()?),
                                f64::from(point.get::<f32>("y").ok()?),
                            ))
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            };
            let bridge = track_overlap_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let Some(object) = bridge.scene.get(&name) else {
                return Ok(false);
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
