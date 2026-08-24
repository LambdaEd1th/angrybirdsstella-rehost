//! Textured-line and rubber-band adapters with distinct native ABIs.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let textured_line_resources = Arc::clone(&resource_runtime);
    let textured_line_bridge = Arc::clone(&render);
    let textured_line_data_root = Arc::clone(&data_root);
    globals.set(
        "drawTexturedLine2D",
        lua.create_function(
            move |_,
                  (sprite, x1, y1, x2, y2, width, _unused_1, _unused_2, _unused_3, _unused_4): (
                String,
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
                let resources = textured_line_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                let geometry = resources.active_atlas_geometry(&sprite).ok_or_else(|| {
                    runtime_error(format!(
                        "drawTexturedLine2D atlas resource '{sprite}' was not found"
                    ))
                })?;
                let bound_region =
                    resources.active_atlas_catalog_region(&sprite, &textured_line_data_root);
                drop(resources);
                let mut bridge = textured_line_bridge
                    .lock()
                    .expect("render bridge lock poisoned");
                if let Some(command) = geometry.native_textured_line_command(
                    sprite,
                    bound_region,
                    bridge.state,
                    (x1, y1),
                    (x2, y2),
                    width,
                ) {
                    bridge.push_render_command(command);
                }
                Ok(())
            },
        )?,
    )?;
    globals.set(
        "drawRubberband",
        lua.create_function(
            move |_, (x1, y1, x2, y2, width, sprite): (f64, f64, f64, f64, f64, String)| {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                let geometry = resources.active_atlas_geometry(&sprite).ok_or_else(|| {
                    runtime_error(format!(
                        "drawRubberband atlas resource '{sprite}' was not found"
                    ))
                })?;
                let bound_region = resources.active_atlas_catalog_region(&sprite, &data_root);
                drop(resources);
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                if let Some(command) = geometry.native_rubberband_command(
                    sprite,
                    bound_region,
                    bridge.state,
                    (x1, y1),
                    (x2, y2),
                    width,
                ) {
                    bridge.push_render_command(command);
                }
                Ok(())
            },
        )?,
    )
}
