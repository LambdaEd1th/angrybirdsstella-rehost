//! Native background and foreground theme-pass entry points.

use crate::*;

pub(super) fn install_background(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawBackgroundNative",
        lua.create_function(move |lua, args: MultiValue| {
            let index = theme_required_f32(&args, 0, "drawBackgroundNative")?;
            let index = native_fcvtzs_f32(index);
            let resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            trace_pass(&bridge, "drawBackgroundNative", false);
            super::camera::prepare_draw(lua, &mut bridge, false)?;
            bridge.draw_theme_pass(
                false,
                (index >= 0).then_some(index as usize),
                &resources,
                &data_root,
            );
            Ok(())
        })?,
    )
}

pub(super) fn install_foreground(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawForegroundNative",
        lua.create_function(move |lua, _: MultiValue| {
            let resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            trace_pass(&bridge, "drawForegroundNative", true);
            super::camera::prepare_draw(lua, &mut bridge, true)?;
            bridge.draw_theme_pass(true, None, &resources, &data_root);
            Ok(())
        })?,
    )
}

fn trace_pass(bridge: &RenderBridge, function_name: &str, foreground: bool) {
    if std::env::var_os("STELLA_TRACE_NATIVE").is_none() {
        return;
    }
    let layer_count = if foreground {
        bridge.theme_foreground_layers.len()
    } else {
        bridge.theme_background_layers.len()
    };
    eprintln!(
        "native {function_name}(layers={layer_count}, scale={:.4}/{:.4})",
        bridge.world_scale, bridge.max_world_scale
    );
}
