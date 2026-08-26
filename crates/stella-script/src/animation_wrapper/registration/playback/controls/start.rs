//! `AnimationWrapper::start` (`sub_100012F18`).

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use super::helpers::{advance_scene, apply_targets};
use crate::*;

pub(super) fn install_start(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    animation_native.set(
        "start",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "start")?;
            let action = native_required_string(&args, 1, "start")?;
            let mode = native_required_string(&args, 2, "start")?;
            let resources = resource_runtime
                .as_ref()
                .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
            let mut runtime = animation_runtime
                .lock()
                .expect("animation runtime lock poisoned");
            let known_action = runtime
                .definitions
                .get(&tag)
                .is_some_and(|definition| definition.actions.contains_key(&action));
            if !known_action {
                return Ok(());
            }
            let duration = runtime
                .actions
                .get(&tag)
                .and_then(|actions| actions.get(&action))
                .copied()
                .map(|duration| f64::from(duration as f32))
                .unwrap_or(0.0);
            let playback = runtime
                .playback
                .entry(tag.clone())
                .or_insert_with(|| AnimationPlayback::detached(action.clone(), duration));
            playback.detached_current = None;
            if let Some(index) = playback.active_control_index(&action) {
                // sub_100410A18 resets an existing named control in place.
                // Its speed and installed callback survive, and its vector
                // position (therefore target precedence) does not change.
                let control = &mut playback.controls[index];
                control.elapsed = 0.0;
                control.previous_elapsed = 0.0;
                control.duration = duration;
                control.paused = false;
                control.playing = true;
                control.finished_pending_removal = false;
            } else {
                playback.controls.push(AnimationControl {
                    action: action.clone(),
                    elapsed: 0.0,
                    previous_elapsed: 0.0,
                    duration,
                    speed: 1.0,
                    paused: false,
                    playing: true,
                    callback_installed: false,
                    finished_pending_removal: false,
                });
            }

            // sub_100012F18 starts the native control, performs the hidden
            // float32 0.00001-second update and a mode-4 forced application,
            // then overwrites the wrapper's shared action/mode fields and
            // installs the completion callback.
            advance_scene(
                &mut runtime,
                resources.as_deref(),
                data_root.as_deref().map(std::path::PathBuf::as_path),
                &tag,
                f64::from(0.00001_f32),
            );
            apply_targets(
                &mut runtime,
                resources.as_deref(),
                data_root.as_deref().map(std::path::PathBuf::as_path),
                &tag,
                4,
            );
            let playback = runtime
                .playback
                .get_mut(&tag)
                .expect("newly started animation playback disappeared");
            playback.current_action = action.clone();
            playback.mode = mode.clone();
            if let Some(index) = playback.active_control_index(&action) {
                playback.controls[index].callback_installed = true;
            }
            if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                eprintln!(
                    "animation-native start tag={tag} action={action} mode={mode} duration={duration:.4} controls={}",
                    playback.controls.len()
                );
            }
            // sub_10001CAF0 returns zero Lua results.
            Ok(())
        })?,
    )
}
