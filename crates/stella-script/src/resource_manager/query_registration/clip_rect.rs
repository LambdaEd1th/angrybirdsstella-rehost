//! `setClipRect`/`getClipRect` members around `sub_1004489EC`.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let set_resources = Arc::clone(&resource_runtime);
    let set_bridge = Arc::clone(&render);
    resource_api.set(
        "setClipRect",
        lua.create_function(move |_, (x, y, width, height): (f64, f64, f64, f64)| {
            // The raw `ffff` dispatcher narrows every slot first; the member
            // then adds in float32 and FCVTZS-converts each stored edge.
            let x = x as f32;
            let y = y as f32;
            let width = width as f32;
            let height = height as f32;
            let edges = [
                native_fcvtzs_f32(x),
                native_fcvtzs_f32(y),
                native_fcvtzs_f32(x + width),
                native_fcvtzs_f32(y + height),
            ];
            set_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .clip_rect = edges;
            set_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .state
                .clip_rect = Some(edges);
            Ok(())
        })?,
    )?;
    resource_api.set(
        "getClipRect",
        lua.create_function(move |_, ()| {
            let [left, top, right, bottom] = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .clip_rect;
            Ok((
                f64::from(left),
                f64::from(top),
                f64::from(right - left),
                f64::from(bottom - top),
            ))
        })?,
    )?;
    Ok(())
}
