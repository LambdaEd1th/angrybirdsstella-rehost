//! Direct LuaState `getWorldPoint`/`getLocalPoint` members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let world_point_bridge = Arc::clone(render);
    globals.set(
        "getWorldPoint",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000550B8 resolves the nullable body before reading slots
            // 2/3, but dereferences its transform only after those reads.
            let name = native_required_string(&args, 0, "getWorldPoint")?;
            let body_transform = body_transform(&world_point_bridge, &name);
            let local_x = native_required_number(&args, 1, "getWorldPoint")? as f32;
            let local_y = native_required_number(&args, 2, "getWorldPoint")? as f32;
            let (position_x, position_y, angle) = body_transform
                .ok_or_else(|| runtime_error(format!("Missing physics body: {name}")))?;
            let (sine, cosine) = angle.sin_cos();

            // FMUL/FNMSUB/FADD and FMUL/FMADD/FADD at 0x100055148.
            let rotated_x = local_x.mul_add(cosine, -(local_y * sine));
            let rotated_y = local_y.mul_add(cosine, local_x * sine);
            Ok((
                f64::from(position_x + rotated_x),
                f64::from(position_y + rotated_y),
            ))
        })?,
    )?;

    let local_point_bridge = Arc::clone(render);
    globals.set(
        "getLocalPoint",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "getLocalPoint")?;
            let body_transform = body_transform(&local_point_bridge, &name);
            let world_x = native_required_number(&args, 1, "getLocalPoint")? as f32;
            let world_y = native_required_number(&args, 2, "getLocalPoint")? as f32;
            let (position_x, position_y, angle) = body_transform
                .ok_or_else(|| runtime_error(format!("Missing physics body: {name}")))?;
            let delta_x = world_x - position_x;
            let delta_y = world_y - position_y;
            let (sine, cosine) = angle.sin_cos();

            // Two FSUBs then the exact mulT(q, world-p) sequence.
            let local_x = delta_x.mul_add(cosine, delta_y * sine);
            let local_y = cosine.mul_add(delta_y, -(delta_x * sine));
            Ok((f64::from(local_x), f64::from(local_y)))
        })?,
    )?;

    Ok(())
}

fn body_transform(render: &Arc<Mutex<RenderBridge>>, name: &str) -> Option<(f32, f32, f32)> {
    let bridge = render.lock().expect("render bridge lock poisoned");
    bridge.scene.get(name).and_then(|object| {
        object
            .has_physics_body()
            .then_some((object.x as f32, object.y as f32, object.angle as f32))
    })
}
