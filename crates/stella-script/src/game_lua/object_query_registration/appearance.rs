//! RenderObject scale, flip, and angle queries.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let scale_bridge = Arc::clone(render);
    globals.set(
        "getScale",
        lua.create_function(move |_, args: MultiValue| {
            // Direct LuaState member sub_1000403E4 pushes +0xBC/+0xC0.
            let name = native_required_string(&args, 0, "getScale")?;
            let bridge = scale_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            Ok((
                f64::from(object.scale_x as f32),
                f64::from(object.scale_y as f32),
            ))
        })?,
    )?;

    let flip_bridge = Arc::clone(render);
    globals.set(
        "isHorizontallyFlipped",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000404F4 returns RenderObjectData+0x139, unrelated to
            // the sign of the reflected Lua scaleX field.
            let name = native_required_string(&args, 0, "isHorizontallyFlipped")?;
            flip_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .scene
                .get(&name)
                .map(|object| object.horizontal_flip)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))
        })?,
    )?;

    for function_name in ["getAngle", "getRotation"] {
        let angle_bridge = Arc::clone(render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                // Both registrations share sub_100054104.
                let name = native_required_string(&args, 0, function_name)?;
                angle_bridge
                    .lock()
                    .expect("render bridge lock poisoned")
                    .scene
                    .get(&name)
                    .map(|object| f64::from(object.angle as f32))
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))
            })?,
        )?;
    }

    Ok(())
}
