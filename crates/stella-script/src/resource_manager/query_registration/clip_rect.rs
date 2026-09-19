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
        lua.create_function(move |_, args: MultiValue| {
            let x = native_required_number(&args, 0, "res.setClipRect")?;
            let y = native_required_number(&args, 1, "res.setClipRect")?;
            let width = native_required_number(&args, 2, "res.setClipRect")?;
            let height = native_required_number(&args, 3, "res.setClipRect")?;
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
        lua.create_function(move |_, _: MultiValue| {
            let [left, top, right, bottom] = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .clip_rect;
            Ok((
                f64::from(left as f32),
                f64::from(top as f32),
                f64::from(right.wrapping_sub(left) as f32),
                f64::from(bottom.wrapping_sub(top) as f32),
            ))
        })?,
    )?;
    Ok(())
}
