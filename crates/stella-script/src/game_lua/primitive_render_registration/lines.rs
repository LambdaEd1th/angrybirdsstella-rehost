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
        lua.create_function(
            move |_,
                  (x1, y1, x2, y2, width, red, green, blue, alpha): (
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
            )| {
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
                if let Some(command) =
                    native_line_command(start, end, width, color, state, clip_rect)
                {
                    bridge.push_rect_command(command);
                }
                Ok(())
            },
        )?,
    )?;

    globals.set(
        "drawRectLines",
        lua.create_function(
            move |_,
                  (left, top, right, bottom, width, red, green, blue, alpha): (
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
            )| {
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
            },
        )?,
    )
}
