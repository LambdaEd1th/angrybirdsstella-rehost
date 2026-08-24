//! Native-order facade for Purple's later PhysicsWorld extension table.

use crate::*;

use super::{
    native_block_registration::install as install_native_block_bindings,
    object_extension_registration::install as install_object_extension_bindings,
    track_joint_registration::install as install_track_joint_bindings,
};

mod gravity_visuals;
mod light_beam;
mod polygon;
mod ray;

pub(crate) fn install_extensions(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    install_track_joint_bindings(lua, globals, Arc::clone(&render))?;
    install_object_extension_bindings(lua, globals, Arc::clone(&render))?;
    install_native_block_bindings(lua, globals, Arc::clone(&render), resources, data_root)?;
    polygon::install(lua, globals)?;
    ray::install(lua, globals, &render)?;
    light_beam::install(lua, globals, &render)?;
    gravity_visuals::install(lua, globals, &render)
}
