//! Native block-extension registration in recovered factory order.

mod collision;
mod factory;
mod queries;
mod rebuild;

use crate::*;

pub(super) type PendingDirtCollisions = Rc<RefCell<Vec<[f64; 5]>>>;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "createNativeBlockExtension",
        lua.create_function(move |lua, args: MultiValue| {
            factory::create(lua, &render, &resources, &data_root, args)
        })?,
    )
}
