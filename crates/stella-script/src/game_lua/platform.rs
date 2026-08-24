//! Platform-facing GameLua entries split by their recovered native owners.

mod installed_apps;
mod misc;
mod registration;
mod sha1;
mod sharing;
mod time;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table};

use crate::RenderBridge;

pub(crate) fn install(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // Relative positions recovered from sub_10002C274. Other GameLua
    // subsystems are interleaved between these groups in the full constructor.
    registration::install_identity(lua, globals)?;
    time::install_get_date(lua, globals)?;
    sharing::install_url(lua, globals, render)?;
    installed_apps::install(lua, globals)?;
    time::install_epoch_conversion(lua, globals)?;
    registration::install_checksum(lua, globals)?;
    misc::install(lua, globals)?;
    sharing::install_screenshot(lua, globals, render)?;
    Ok(())
}

#[cfg(test)]
pub(crate) use sha1::sha1_upper_hex;
