//! `drawPolygon` / `sub_100043F28`, registered at `0x10002DA48`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawPolygon",
        lua.create_function(
            move |_,
                  (vertices, offset_x, offset_y, red, green, blue, alpha): (
                mlua::Table,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
            )| {
                let mut points = Vec::new();
                for value in vertices.sequence_values::<Value>() {
                    if let Value::Table(point) = value? {
                        points.push((point.get::<f64>("x")?, point.get::<f64>("y")?));
                    }
                }
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                let state = bridge.state;
                for command in native_polygon_commands(
                    &points,
                    offset_x,
                    offset_y,
                    [red, green, blue, alpha],
                    state,
                ) {
                    bridge.push_rect_command(command);
                }
                Ok(())
            },
        )?,
    )
}
