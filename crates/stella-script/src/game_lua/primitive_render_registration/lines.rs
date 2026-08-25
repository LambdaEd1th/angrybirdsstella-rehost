//! Line members `sub_10004DC44` and `sub_10004DC8C`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let line_bridge = Arc::clone(&render);
    globals.set(
        "drawLine2D",
        lua.create_function(move |_, args: MultiValue| {
            // Shared generated adapter sub_100084918 validates nine
            // exact NUMBER slots and ignores trailing stack values.
            let x1 = native_required_number(&args, 0, "drawLine2D")?;
            let y1 = native_required_number(&args, 1, "drawLine2D")?;
            let x2 = native_required_number(&args, 2, "drawLine2D")?;
            let y2 = native_required_number(&args, 3, "drawLine2D")?;
            let width = native_required_number(&args, 4, "drawLine2D")?;
            let red = native_required_number(&args, 5, "drawLine2D")?;
            let green = native_required_number(&args, 6, "drawLine2D")?;
            let blue = native_required_number(&args, 7, "drawLine2D")?;
            let alpha = native_required_number(&args, 8, "drawLine2D")?;
            let start = (
                f64::from(native_fcvtzs_f32(x1 as f32)),
                f64::from(native_fcvtzs_f32(y1 as f32)),
            );
            let end = (
                f64::from(native_fcvtzs_f32(x2 as f32)),
                f64::from(native_fcvtzs_f32(y2 as f32)),
            );
            let width = f64::from(native_fcvtzs_f32(width as f32));
            let color = [red, green, blue, alpha].map(native_packed_color_channel);
            let mut bridge = line_bridge.lock().expect("render bridge lock poisoned");
            let clip_rect = bridge.state.clip_rect;
            let state = bridge.state;
            if let Some(command) = native_line_command(start, end, width, color, state, clip_rect) {
                bridge.push_rect_command(command);
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "drawRectLines",
        lua.create_function(move |_, args: MultiValue| {
            // drawRectLines is registered through the same strict
            // sub_1000848B0/sub_100084918 wrapper as drawLine2D.
            let left = native_required_number(&args, 0, "drawRectLines")?;
            let top = native_required_number(&args, 1, "drawRectLines")?;
            let right = native_required_number(&args, 2, "drawRectLines")?;
            let bottom = native_required_number(&args, 3, "drawRectLines")?;
            let width = native_required_number(&args, 4, "drawRectLines")?;
            let red = native_required_number(&args, 5, "drawRectLines")?;
            let green = native_required_number(&args, 6, "drawRectLines")?;
            let blue = native_required_number(&args, 7, "drawRectLines")?;
            let alpha = native_required_number(&args, 8, "drawRectLines")?;
            let left = f64::from(native_fcvtzs_f32(left as f32));
            let top = f64::from(native_fcvtzs_f32(top as f32));
            let right = f64::from(native_fcvtzs_f32(right as f32));
            let bottom = f64::from(native_fcvtzs_f32(bottom as f32));
            let width = f64::from(native_fcvtzs_f32(width as f32));
            let color = [red, green, blue, alpha].map(native_packed_color_channel);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let clip_rect = bridge.state.clip_rect;
            let state = bridge.state;
            for (start, end) in [
                ((left, top), (right, top)),
                ((left, top), (left, bottom)),
                ((left, bottom), (right, bottom)),
                ((right, top), (right, bottom)),
            ] {
                if let Some(command) =
                    native_line_command(start, end, width, color, state, clip_rect)
                {
                    bridge.push_rect_command(command);
                }
            }
            Ok(())
        })?,
    )
}
