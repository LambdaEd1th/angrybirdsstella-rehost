//! GL-context state members registered at `0x10002D968..0x10002DB28`.

use crate::*;

pub(super) fn install_world_scale(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setWorldScale",
        lua.create_function(move |lua, args: MultiValue| {
            let scale = f64::from(native_required_number(&args, 0, "setWorldScale")? as f32);
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setWorldScale({scale})");
            }
            render
                .lock()
                .expect("render bridge lock poisoned")
                .world_scale = scale;
            game_environment(lua)?.set("worldScale", scale)?;
            Ok(())
        })?,
    )
}

pub(super) fn install_render_state(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setRenderState",
        lua.create_function(move |_, args: MultiValue| {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // sub_100044CFC commits nested arity groups. A later bad group
            // leaves prior groups committed but never half-writes a pair.
            let argument_count = args.len();
            let required_number = |index: usize| {
                native_required_number(&args, index, "setRenderState")
                    .map(|value| f64::from(value as f32))
            };
            if argument_count >= 2 {
                let translate_x = required_number(0)?;
                let translate_y = required_number(1)?;
                bridge.state.translate_x = translate_x;
                bridge.state.translate_y = translate_y;
                if argument_count >= 4 {
                    let scale_x = required_number(2)?;
                    let scale_y = required_number(3)?;
                    bridge.state.scale_x = scale_x;
                    bridge.state.scale_y = scale_y;
                    if argument_count >= 5 {
                        bridge.state.angle = required_number(4)?;
                        bridge.state.matrix = None;
                        if argument_count >= 7 {
                            let pivot_x = required_number(5)?;
                            let pivot_y = required_number(6)?;
                            bridge.state.pivot_x = pivot_x;
                            bridge.state.pivot_y = pivot_y;
                            if argument_count >= 8 {
                                bridge.state.alpha = required_number(7)?;
                            }
                        }
                    }
                }
            }
            Ok(())
        })?,
    )
}

pub(super) fn install_alpha(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_setAlpha",
        lua.create_function(move |_, args: MultiValue| {
            let alpha = f64::from(native_required_number(&args, 0, "native_setAlpha")? as f32);
            render
                .lock()
                .expect("render bridge lock poisoned")
                .state
                .alpha = alpha;
            Ok(())
        })?,
    )
}
