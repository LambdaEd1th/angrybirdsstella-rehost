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

pub(crate) struct PlatformRuntimes {
    pub(crate) url_requests: UrlRequestRuntime,
    pub(crate) installed_apps: InstalledAppsRuntime,
}

pub(crate) fn install(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<PlatformRuntimes> {
    // Relative positions recovered from sub_10002C274. Other GameLua
    // subsystems are interleaved between these groups in the full constructor.
    registration::install_identity(lua, globals)?;
    time::install_get_date(lua, globals)?;
    let url_requests = sharing::install_url(lua, globals, render)?;
    let installed_apps = installed_apps::install(lua, globals)?;
    time::install_epoch_conversion(lua, globals)?;
    registration::install_checksum(lua, globals)?;
    misc::install(lua, globals)?;
    sharing::install_screenshot(lua, globals, render)?;
    Ok(PlatformRuntimes {
        url_requests,
        installed_apps,
    })
}

pub(crate) use installed_apps::{
    InstalledAppsRuntime, dispatch_completion as dispatch_installed_apps,
};
pub(crate) use sharing::{UrlRequestRuntime, dispatch_url_completions};

#[cfg(test)]
pub(crate) use sha1::sha1_upper_hex;
#[cfg(test)]
pub(crate) use sharing::set_screenshot_sequence_for_test;
