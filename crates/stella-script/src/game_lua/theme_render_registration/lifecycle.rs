//! ThemeManager reset and the optional level-load particle pre-roll.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "updateThemeParticlesNative",
        lua.create_function(move |_, args: MultiValue| {
            let delta = native_required_number(&args, 0, "updateThemeParticlesNative")? as f32;
            let resources = resources.lock().expect("resource runtime lock poisoned");
            render
                .lock()
                .expect("render bridge lock poisoned")
                .advance_theme_particles_only(delta, &resources, &data_root);
            Ok(())
        })?,
    )
}
