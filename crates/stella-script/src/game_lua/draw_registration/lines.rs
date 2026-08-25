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
        lua.create_function(move |_, args: MultiValue| {
            // sub_100084A9C requires an exact STRING followed by nine
            // exact NUMBER slots and ignores any trailing stack values.
            let sprite = native_required_string(&args, 0, "drawTexturedLine2D")?;
            let x1 = native_required_number(&args, 1, "drawTexturedLine2D")?;
            let y1 = native_required_number(&args, 2, "drawTexturedLine2D")?;
            let x2 = native_required_number(&args, 3, "drawTexturedLine2D")?;
            let y2 = native_required_number(&args, 4, "drawTexturedLine2D")?;
            let width = native_required_number(&args, 5, "drawTexturedLine2D")?;
            let _unused_1 = native_required_number(&args, 6, "drawTexturedLine2D")?;
            let _unused_2 = native_required_number(&args, 7, "drawTexturedLine2D")?;
            let _unused_3 = native_required_number(&args, 8, "drawTexturedLine2D")?;
            let _unused_4 = native_required_number(&args, 9, "drawTexturedLine2D")?;
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
        })?,
    )?;
    globals.set(
        "drawRubberband",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000897A4 validates five exact NUMBER slots followed
            // by an exact STRING and leaves trailing values untouched.
            let x1 = native_required_number(&args, 0, "drawRubberband")?;
            let y1 = native_required_number(&args, 1, "drawRubberband")?;
            let x2 = native_required_number(&args, 2, "drawRubberband")?;
            let y2 = native_required_number(&args, 3, "drawRubberband")?;
            let width = native_required_number(&args, 4, "drawRubberband")?;
            let sprite = native_required_string(&args, 5, "drawRubberband")?;
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
        })?,
    )
}
