//! Water members registered contiguously at `0x10002CAE0..0x10002CB70`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // Preserve Purple's exact order: bird drag, object drag, color, then the
    // independent additional-gravity member at address 0x1000311AC.
    for (function_name, field) in [
        ("native_setBirdWaterDrag", 1_u8),
        ("native_setObjectWaterDrag", 0_u8),
    ] {
        let water_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let value = f64::from(native_required_number(&args, 0, function_name)? as f32);
                let mut bridge = water_bridge.lock().expect("render bridge lock poisoned");
                match field {
                    0 => bridge.object_water_drag = value,
                    1 => bridge.bird_water_drag = value,
                    _ => unreachable!(),
                }
                Ok(())
            })?,
        )?;
    }

    let water_color_bridge = Arc::clone(&render);
    globals.set(
        "native_setWaterColor",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100088BC0 reads four exact NUMBER slots and narrows every
            // value before the member is entered. Decode all four first.
            let color = [
                f64::from(native_required_number(&args, 0, "native_setWaterColor")? as f32),
                f64::from(native_required_number(&args, 1, "native_setWaterColor")? as f32),
                f64::from(native_required_number(&args, 2, "native_setWaterColor")? as f32),
                f64::from(native_required_number(&args, 3, "native_setWaterColor")? as f32),
            ];
            let mut bridge = water_color_bridge
                .lock()
                .expect("render bridge lock poisoned");
            bridge.water_color = color;
            Ok(())
        })?,
    )?;

    let gravity_bridge = Arc::clone(&render);
    globals.set(
        "native_setAdditionalBirdGravity",
        lua.create_function(move |_, args: MultiValue| {
            let value =
                f64::from(
                    native_required_number(&args, 0, "native_setAdditionalBirdGravity")? as f32,
                );
            let mut bridge = gravity_bridge.lock().expect("render bridge lock poisoned");
            // sub_1000311AC writes byte offset +0x224. This is independent
            // from the water-color vector at +0x540..+0x54C; the decimal
            // offset 548 must not be confused with hexadecimal 0x548.
            bridge.additional_bird_gravity = value;
            Ok(())
        })?,
    )?;
    Ok(())
}
