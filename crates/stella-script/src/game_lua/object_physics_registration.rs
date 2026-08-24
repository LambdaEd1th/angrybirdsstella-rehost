//! Installation order for Purple's scene-object physics registration cluster.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // Keep the executable's constructor order while delegating to the same
    // functional boundaries exposed by b2Body/b2Fixture and RenderObjectData.
    install_object_motion_bindings(lua, globals, Arc::clone(&render))?;
    install_object_body_bindings(lua, globals, Arc::clone(&render))?;
    install_object_material_bindings(lua, globals, render, resources, data_root)
}
