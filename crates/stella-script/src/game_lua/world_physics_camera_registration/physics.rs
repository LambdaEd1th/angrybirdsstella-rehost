//! Physics gates registered at `0x10002CF20` and `0x10002D7F8..0x10002D878`.

use crate::*;

pub(super) fn install_aiming_aid(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "enableAimingAid",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "enableAimingAid")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .aiming_aid_enabled = enabled;
            Ok(())
        })?,
    )
}

pub(super) fn install_core(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let simulation_scale_bridge = Arc::clone(&render);
    globals.set(
        "setPhysicsSimulationScale",
        lua.create_function(move |_, args: MultiValue| {
            let scale =
                f64::from(native_required_number(&args, 0, "setPhysicsSimulationScale")? as f32);
            simulation_scale_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .physics_simulation_scale = scale;
            Ok(())
        })?,
    )?;

    let enabled_bridge = Arc::clone(&render);
    globals.set(
        "setPhysicsEnabled",
        lua.create_function(move |_, args: MultiValue| {
            // `sub_100041ABC` reads the first slot through Lua's strict boolean
            // accessor and only probes the optional lock name when argc >= 2.
            let enabled = native_required_boolean(&args, 0, "setPhysicsEnabled")?;
            let lock_name = if args.len() >= 2 {
                native_required_string(&args, 1, "setPhysicsEnabled")?
            } else {
                String::new()
            };
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setPhysicsEnabled({enabled}, {lock_name:?})");
            }
            enabled_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .set_physics_enabled(enabled, lock_name);
            Ok(())
        })?,
    )?;

    globals.set(
        "unlockPhysicsLock",
        lua.create_function(move |_, args: MultiValue| {
            let lock_name = native_required_string(&args, 0, "unlockPhysicsLock")?;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native unlockPhysicsLock({lock_name:?})");
            }
            render
                .lock()
                .expect("render bridge lock poisoned")
                .unlock_physics_lock(&lock_name);
            Ok(())
        })?,
    )
}
