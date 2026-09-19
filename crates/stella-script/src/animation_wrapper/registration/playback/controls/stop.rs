//! Named/scene/global stop paths (`sub_100013720` and `sub_100012910`).

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use super::helpers::apply_targets;
use crate::*;

pub(super) fn install_stop(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    animation_native.set(
        "stop",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "stop")?;
            let action = native_required_string(&args, 1, "stop")?;
            let resources = resource_runtime
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = animation_runtime
                .lock()
                .expect("animation runtime lock poisoned");
            if action.is_empty() {
                let control_count = runtime
                    .playback
                    .get(&tag)
                    .map_or(0, |playback| playback.controls.len());
                for index in 0..control_count {
                    if let Some(control) = runtime
                        .playback
                        .get_mut(&tag)
                        .and_then(|playback| playback.controls.get_mut(index))
                    {
                        control.elapsed = 0.0;
                        control.previous_elapsed = 0.0;
                        control.playing = false;
                        control.paused = false;
                        control.finished_pending_removal = false;
                    }
                    apply_targets(
                        &mut runtime,
                        resources.as_deref(),
                        data_root.as_deref().map(std::path::PathBuf::as_path),
                        &tag,
                        2,
                    );
                }
            } else {
                let (found, removed_current) =
                    runtime
                        .playback
                        .get_mut(&tag)
                        .map_or((false, None), |playback| {
                            let Some(index) = playback.active_control_index(&action) else {
                                return (false, None);
                            };
                            let mut control = playback.controls.swap_remove(index);
                            control.elapsed = 0.0;
                            control.previous_elapsed = 0.0;
                            control.playing = false;
                            control.paused = false;
                            control.finished_pending_removal = false;
                            let current =
                                (control.action == playback.current_action).then_some(control);
                            (true, current)
                        });
                if let Some(control) = removed_current
                    && let Some(playback) = runtime.playback.get_mut(&tag)
                {
                    playback.detached_current = Some(control);
                }
                if found {
                    apply_targets(
                        &mut runtime,
                        resources.as_deref(),
                        data_root.as_deref().map(std::path::PathBuf::as_path),
                        &tag,
                        2,
                    );
                }
            }
            Ok(())
        })?,
    )
}

pub(super) fn install_stop_all(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    animation_native.set(
        "stopAll",
        lua.create_function(move |_, _: MultiValue| {
            let resources = resource_runtime
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = animation_runtime
                .lock()
                .expect("animation runtime lock poisoned");
            stop_all_native(
                &mut runtime,
                resources.as_deref(),
                data_root.as_deref().map(std::path::PathBuf::as_path),
            );
            Ok(())
        })?,
    )
}

pub(in crate::animation_wrapper::registration) fn stop_all_native(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&std::path::Path>,
) {
    // sub_100012910 walks live entities, not the wrapper's retained-control
    // map: orphan controls surviving a root flush are not stopped again.
    let tags = runtime.definitions.keys().cloned().collect::<Vec<_>>();
    for tag in tags {
        let control_count = runtime
            .playback
            .get(&tag)
            .map_or(0, |playback| playback.controls.len());
        for index in 0..control_count {
            let control = &mut runtime
                .playback
                .get_mut(&tag)
                .expect("animation playback disappeared")
                .controls[index];
            control.elapsed = 0.0;
            control.previous_elapsed = 0.0;
            control.playing = false;
            control.paused = false;
            control.finished_pending_removal = false;
            apply_targets(runtime, resources, data_root, &tag, 2);
        }
    }
}
