//! `checkJointLimits`/`handleJointLimits` (`sub_100054CD4`/`sub_100054DAC`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let handle_joint_bridge = Arc::clone(&render);
    globals.set(
        "handleJointLimits",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "handleJointLimits")?;
            let enabled = native_required_boolean(&args, 1, "handleJointLimits")?;
            handle_joint_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .handle_joint_limit_boundary(&name, enabled);
            Ok(())
        })?,
    )?;
    globals.set(
        "checkJointLimits",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "checkJointLimits")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .handle_joint_limit_boundary(&name, false);
            Ok(())
        })?,
    )?;
    Ok(())
}
