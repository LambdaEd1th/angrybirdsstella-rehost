//! DrawablePolygon `makeRay` member (`sub_10004CE5C`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let make_ray_bridge = Arc::clone(render);
    globals.set(
        "makeRay",
        lua.create_function(move |_, args: MultiValue| {
            // The four floats are RGBA, not line endpoints.
            let name = native_required_string(&args, 0, "makeRay")?;
            let color = [
                f64::from(native_required_number(&args, 1, "makeRay")? as f32),
                f64::from(native_required_number(&args, 2, "makeRay")? as f32),
                f64::from(native_required_number(&args, 3, "makeRay")? as f32),
                f64::from(native_required_number(&args, 4, "makeRay")? as f32),
            ];
            let mut bridge = make_ray_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_10004CE5C copies RenderObjectData+0x168 and the position at
            // +0xA4/+0xA8 before `_M_insert_unique`. The retained drawable is
            // therefore independent of later body position/angle changes.
            if object.ray.is_none() {
                object.ray = Some(Arc::new(DrawablePolygonState {
                    vertices: object.collision_local_vertices(),
                    x: f64::from(object.x as f32),
                    y: f64::from(object.y as f32),
                    color,
                }));
            }
            Ok(())
        })?,
    )
}
