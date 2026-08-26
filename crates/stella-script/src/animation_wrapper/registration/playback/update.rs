//! Frame advancement and queued playback-event dispatch.

mod advance;
mod apply;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::*;

#[cfg(test)]
use advance::advance_native_once;
pub(super) use advance::{advance_native_scene, advance_native_scene_with_resources};
pub(super) use apply::{apply_native_targets, apply_native_targets_with_resources};

#[cfg(test)]
pub(in super::super) fn install_update(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
) -> LuaResult<()> {
    install_update_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        None,
        None,
    )
}

pub(in super::super) fn install_update_with_resources(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
) -> LuaResult<()> {
    install_update_inner(
        lua,
        animation_native,
        animation_runtime,
        animation_callbacks,
        Some(resource_runtime),
        Some(data_root),
    )
}

fn install_update_inner(
    lua: &Lua,
    animation_native: &Table,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    animation_callbacks: Table,
    resource_runtime: Option<Arc<Mutex<ResourceRuntime>>>,
    data_root: Option<Arc<std::path::PathBuf>>,
) -> LuaResult<()> {
    let runtime = Arc::clone(&animation_runtime);
    let resources = resource_runtime;
    let sprite_data_root = data_root;
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "update",
        lua.create_function(move |_, args: MultiValue| {
            let delta_time = f64::from(native_required_number(&args, 0, "update")? as f32);
            let event_groups;
            {
                let resources = resources
                    .as_ref()
                    .map(|resources| resources.lock().expect("resource runtime lock poisoned"));
                let mut runtime = runtime.lock().expect("animation runtime lock poisoned");
                let tags = runtime.playback.keys().cloned().collect::<Vec<_>>();
                for tag in tags {
                    if let (Some(resources), Some(data_root)) =
                        (resources.as_deref(), sprite_data_root.as_deref())
                    {
                        advance_native_scene_with_resources(
                            &mut runtime,
                            resources,
                            data_root,
                            &tag,
                            delta_time,
                        );
                    } else {
                        advance_native_scene(&mut runtime, &tag, delta_time);
                    }
                }

                // sub_100016FE4 drains a snapshot of each component's event
                // vector. Callback-generated events are appended to the now
                // empty live queue and wait for the next wrapper update.
                event_groups = std::mem::take(&mut runtime.pending_event_tags)
                    .into_iter()
                    .filter_map(|tag| {
                        runtime
                            .pending_events
                            .remove(&tag)
                            .map(|events| (tag, events))
                    })
                    .collect::<Vec<_>>();
            }
            for (tag, events) in event_groups {
                for event in events {
                    let action = runtime
                        .lock()
                        .expect("animation runtime lock poisoned")
                        .playback
                        .get(&tag)
                        .map(|playback| playback.current_action.clone())
                        .unwrap_or_default();
                    if std::env::var_os("STELLA_TRACE_ANIMATION").is_some() {
                        eprintln!("animation-native event tag={tag} event={}", event.name);
                    }
                    if let Value::Function(callback) = callbacks.raw_get::<Value>(tag.as_str())? {
                        // sub_100016FE4 pushes exactly six values in this
                        // order and reads the component's current action at
                        // dispatch time rather than when the event was queued.
                        callback.call::<()>((
                            tag.clone(),
                            action,
                            event.name,
                            event.integer,
                            event.number,
                            event.text,
                        ))?;
                    }
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
