//! Lua-owned `ThemeSystem` publication (`sub_10008AD30`).

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let theme_system = lua.create_table()?;
    install_force_spawn(
        lua,
        &theme_system,
        "spawnBGLayerParticles",
        false,
        Arc::clone(&render),
        Arc::clone(&resources),
        Arc::clone(&data_root),
    )?;
    install_force_spawn(
        lua,
        &theme_system,
        "spawnFGLayerParticles",
        true,
        render,
        resources,
        data_root,
    )?;
    globals.set("themeSystem", theme_system)
}

fn install_force_spawn(
    lua: &Lua,
    theme_system: &mlua::Table,
    function_name: &'static str,
    foreground: bool,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    theme_system.set(
        function_name,
        lua.create_function(move |_, args: MultiValue| {
            // The generated member reads stack index -1, so both the native
            // colon form and a direct function call select the final value.
            let last = args.len().saturating_sub(1);
            let spawner_id =
                native_fcvtzs_f32(native_required_number(&args, last, function_name)? as f32);
            let resources = resources.lock().expect("resource runtime lock poisoned");
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let layer_index = {
                let layers = if foreground {
                    &bridge.theme_foreground_layers
                } else {
                    &bridge.theme_background_layers
                };
                layers
                    .iter()
                    .position(|layer| layer.spawner_id == spawner_id)
                    .map(|index| index as i32 + 1)
            };
            let Some(layer_index) = layer_index else {
                return Ok(());
            };
            let RenderBridge {
                particle_random,
                theme_background_particles,
                theme_foreground_particles,
                ..
            } = &mut *bridge;
            let particles = if foreground {
                theme_foreground_particles
            } else {
                theme_background_particles
            };
            particles.force_spawn_layer(layer_index, particle_random, &resources, &data_root);
            Ok(())
        })?,
    )
}
