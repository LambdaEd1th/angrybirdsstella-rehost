//! Safe host routing for bundle, data and AppData files.

use std::path::{Component, Path, PathBuf};

use mlua::Error as LuaError;

use crate::ScriptError;

pub(crate) fn resolve_script(data_root: &Path, requested: &str) -> Result<PathBuf, ScriptError> {
    resolve_file(
        data_root,
        requested,
        &[
            Path::new(""),
            Path::new("scripts"),
            Path::new("scripts_common"),
            Path::new("levels"),
        ],
    )
}

pub(crate) fn resolve_data_file(data_root: &Path, requested: &str) -> Result<PathBuf, ScriptError> {
    resolve_file(
        data_root,
        requested,
        &[
            Path::new(""),
            Path::new("config"),
            Path::new("localization"),
        ],
    )
}

/// Resolve a path exactly as the native bundle stream does, while keeping the
/// result inside the extracted runtime data directory.  The native
/// `BundleInputStream::Impl::constructPath` strips one leading slash and then
/// joins the request to the bundle root; unlike the text/script helpers it
/// does not restrict callers to a short list of subdirectories.  Keeping this
/// resolver separate prevents broad bundle access from changing the lookup
/// order of regular script and resource loads.
pub(crate) fn resolve_bundle_file(
    data_root: &Path,
    requested: &str,
) -> Result<PathBuf, ScriptError> {
    resolve_file(data_root, requested, &[Path::new("")])
}

pub(crate) fn app_data_root(data_root: &Path) -> PathBuf {
    data_root.parent().unwrap_or(data_root).join("appdata")
}

pub(crate) fn app_data_path(data_root: &Path, requested: &str) -> Result<PathBuf, ScriptError> {
    let relative = Path::new(requested.trim_start_matches('/'));
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ScriptError::UnsafePath(requested.to_owned()));
    }
    Ok(app_data_root(data_root).join(relative))
}

pub(crate) fn legacy_json_app_data_path(data_root: &Path, requested: &str) -> Option<PathBuf> {
    let file_name = Path::new(requested).file_name()?.to_str()?;
    let path = app_data_path(data_root, &format!("{file_name}.json")).ok()?;
    path.is_file().then_some(path)
}

pub(crate) fn with_lua_extension(mut requested: String) -> String {
    if Path::new(&requested).extension().is_none() {
        requested.push_str(".lua");
    }
    requested
}

pub(crate) fn resolve_file(
    data_root: &Path,
    requested: &str,
    prefixes: &[&Path],
) -> Result<PathBuf, ScriptError> {
    let normalized = requested
        .trim_start_matches('/')
        .trim_start_matches("data/");
    let relative = Path::new(normalized);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ScriptError::UnsafePath(requested.to_owned()));
    }
    for prefix in prefixes {
        let candidate = data_root.join(prefix).join(relative);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(ScriptError::NotFound(requested.to_owned()))
}

pub(crate) fn runtime_error(error: impl std::fmt::Display) -> LuaError {
    LuaError::RuntimeError(error.to_string())
}
