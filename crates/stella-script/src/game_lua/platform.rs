//! Platform-facing GameLua entries split by their recovered native owners.

mod installed_apps;
mod misc;
mod registration;
mod sha1;
mod sharing;
mod time;
mod uuid;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table};

use crate::{ApplicationEventScheduler, RenderBridge};

pub(crate) struct PlatformRuntimes {
    pub(crate) url_requests: UrlRequestRuntime,
    pub(crate) installed_apps: InstalledAppsRuntime,
}

pub(crate) fn install(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<PlatformRuntimes> {
    // Relative positions recovered from sub_10002C274. Other GameLua
    // subsystems are interleaved between these groups in the full constructor.
    registration::install_identity(lua, globals)?;
    time::install_get_date(lua, globals)?;
    let url_requests = sharing::install_url(lua, globals, render, application_events)?;
    let installed_apps = installed_apps::install(lua, globals, Arc::clone(render))?;
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
    InstalledAppsRuntime, can_open_url, dispatch_completion as dispatch_installed_apps,
    normalized_url_scheme,
};
pub(crate) use sharing::{UrlRequestRuntime, dispatch_url_completion};

#[cfg(test)]
pub(crate) use sha1::sha1_upper_hex;
pub(crate) use sha1::{sha1_digest, upper_hex};
#[cfg(test)]
pub(crate) use sharing::set_screenshot_sequence_for_test;
pub(crate) use uuid::generate_uuid_v4;
