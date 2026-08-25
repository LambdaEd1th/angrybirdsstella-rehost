//! Native-order façade for GameLua audio members at `0x10002EE48..0x10002EEAC`.

use crate::*;

mod channel;
mod playback;
mod volume;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    channel::install(
        lua,
        globals,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?; // 0x10002EE48
    playback::install_play(
        lua,
        globals,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?; // 0x10002EE6C
    volume::install(
        lua,
        globals,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?; // 0x10002EE8C
    playback::install_stop(lua, globals, resource_runtime, audio_runtime) // 0x10002EEAC
}

fn optional_number(
    args: &MultiValue,
    index: usize,
    default: f32,
    function: &str,
) -> LuaResult<f32> {
    match args.iter().nth(index) {
        None | Some(Value::Nil) => Ok(default),
        Some(_) => Ok(native_required_number(args, index, function)? as f32),
    }
}

fn optional_boolean(
    args: &MultiValue,
    index: usize,
    default: bool,
    function: &str,
) -> LuaResult<bool> {
    match args.iter().nth(index) {
        None | Some(Value::Nil) => Ok(default),
        Some(_) => native_required_boolean(args, index, function),
    }
}
