//! Framebuffer clear member `sub_100044E84`, registered at `0x10002DB88`.

use crate::*;

pub(super) fn install_clear_screen(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "clearScreen",
        lua.create_function(move |_, _: MultiValue| {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.state = RenderState::default();
            let color = bridge
                .background_color
                .map(|channel| f64::from(channel) / 255.0);
            // Native clearScreen resets the Context state then invokes actual
            // glClear, not a quad under the retained perspective projection.
            // The portable command is an opaque screen-space overwrite. Keep
            // its projection absent without changing the separate projection
            // used by subsequent ordinary draws.
            let order = bridge.allocate_draw_order();
            bridge.rect_commands.push(RectRenderCommand {
                projection_3d: None,
                order,
                red: color[0],
                green: color[1],
                blue: color[2],
                alpha: 1.0,
                left: -32000.0,
                top: -32000.0,
                right: 32000.0,
                bottom: 32000.0,
                color_program: ColorProgram::Plain,
                vertices: Some(vec![
                    [-32000.0, -32000.0],
                    [32000.0, -32000.0],
                    [-32000.0, 32000.0],
                    [32000.0, 32000.0],
                ]),
                mesh_topology: ColorMeshTopology::TriangleStrip,
                clip_rect: None,
            });
            Ok(())
        })?,
    )
}
