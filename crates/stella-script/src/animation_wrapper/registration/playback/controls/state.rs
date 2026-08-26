//! Playback queries and retained-control state mutation.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use super::helpers::apply_targets;
use crate::*;

pub(super) fn install_is_playing(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    animation_native.set(
        "isPlaying",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "isPlaying")?;
            Ok(animation_runtime
                .lock()
                .expect("animation runtime lock poisoned")
                .playback
                .get(&tag)
                .and_then(AnimationPlayback::current_control)
                .is_some_and(|control| control.playing && !control.paused))
        })?,
    )
}

pub(super) fn install_pause_resume(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    for (method, paused) in [("pause", true), ("resume", false)] {
        let runtime = Arc::clone(&animation_runtime);
        animation_native.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                let tag = native_required_string(&args, 0, method)?;
                if let Some(playback) = runtime
                    .lock()
                    .expect("animation runtime lock poisoned")
                    .playback
                    .get_mut(&tag)
                    && let Some(control) = playback.current_control_mut()
                {
                    control.paused = paused;
                    control.playing = !paused;
                    if !paused {
                        // sub_100013B9C restores state 3 even when the tag's
                        // retained control was previously stopped/detached.
                        control.finished_pending_removal = false;
                    }
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

pub(super) fn install_set_speed(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
) -> LuaResult<()> {
    animation_native.set(
        "setSpeed",
        lua.create_function(move |_, args: MultiValue| {
            // setSpeed and seek share generated adapter sub_10001C8C0:
            // slot 1 is a strict string and slot 2 a strict NUMBER.
            let tag = native_required_string(&args, 0, "setSpeed")?;
            let speed = f64::from(native_required_number(&args, 1, "setSpeed")? as f32);
            if let Some(playback) = animation_runtime
                .lock()
                .expect("animation runtime lock poisoned")
                .playback
                .get_mut(&tag)
                && let Some(control) = playback.current_control_mut()
            {
                // sub_100013D08 stores the float verbatim, including negative
                // values used for reverse playback.
                control.speed = speed;
            }
            Ok(())
        })?,
    )
}

pub(super) fn install_seek(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    animation_native.set(
        "seek",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "seek")?;
            let time = f64::from(native_required_number(&args, 1, "seek")? as f32);
            let resources = resource_runtime
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = animation_runtime
                .lock()
                .expect("animation runtime lock poisoned");
            let found = if let Some(control) = runtime
                .playback
                .get_mut(&tag)
                .and_then(AnimationPlayback::current_control_mut)
            {
                // sub_10001396C resolves the scene and sub_10040E798 stores
                // the adapter's float32 seek time verbatim before a mode-2
                // forced application of every discrete state.
                control.elapsed = time;
                true
            } else {
                false
            };
            if found {
                apply_targets(
                    &mut runtime,
                    resources.as_deref(),
                    data_root.as_deref().map(std::path::PathBuf::as_path),
                    &tag,
                    2,
                );
            }
            Ok(())
        })?,
    )
}
