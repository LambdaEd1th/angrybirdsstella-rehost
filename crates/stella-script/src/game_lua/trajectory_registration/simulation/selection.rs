//! Selected simulation-bird binding (`setSelectedBirdDuringSimulation`).

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_selected(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setSelectedBirdDuringSimulation",
        lua.create_function(move |_, args: MultiValue| {
            let selected = native_required_string(&args, 0, "setSelectedBirdDuringSimulation")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .selected_simulation_bird = Some(selected);
            Ok(())
        })?,
    )
}
