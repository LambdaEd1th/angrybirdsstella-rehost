//! Hand-written `drawBoxNative` member (`sub_100051BBC`).

use crate::*;

mod arguments;
mod background;
mod layout;
mod resource;

use arguments::BoxDrawArguments;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawBoxNative",
        lua.create_function(move |_, args: MultiValue| {
            let arguments = BoxDrawArguments::parse(&args)?;
            let resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let Some(commands) = layout::commands(&arguments, &resources, &data_root) else {
                return Ok(());
            };
            drop(resources);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.extend_render_commands(commands);
            background::submit(&arguments, &mut bridge);
            Ok(())
        })?,
    )
}
