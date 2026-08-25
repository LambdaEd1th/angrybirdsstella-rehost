//! Lua adapters for native standalone and composite clip construction.

use std::time::Duration;

use super::super::legacy_usage;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let create_audio_resources = Arc::clone(&resource_runtime);
    let create_audio_runtime = Arc::clone(&audio_runtime);
    let create_audio_root = Arc::clone(&data_root);
    resource_api.set(
        "createAudio",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100447984 accepts path, logical name and an optional
            // streaming flag (default true); lifetime lookup is keyed by the
            // logical second string.
            let path = args
                .front()
                .and_then(value_string)
                .ok_or_else(|| runtime_error("createAudio argument 1 must be string"))?;
            let name = args
                .iter()
                .nth(1)
                .and_then(value_string)
                .ok_or_else(|| runtime_error("createAudio argument 2 must be string"))?;
            let _streaming = if args.len() >= 3 {
                match args.iter().nth(2) {
                    Some(Value::Boolean(value)) => *value,
                    _ => return Err(runtime_error("createAudio argument 3 must be boolean")),
                }
            } else {
                true
            };
            let resolved = {
                let resources = create_audio_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                let prefixed = if resources.path.is_empty() {
                    None
                } else {
                    Some(format!(
                        "{}/{}",
                        resources.path.trim_end_matches('/'),
                        path.trim_start_matches('/')
                    ))
                };
                prefixed
                    .as_deref()
                    .and_then(|path| legacy_usage::audio_file_path(&create_audio_root, path, false))
                    .or_else(|| legacy_usage::audio_file_path(&create_audio_root, &path, false))
            }
            .ok_or_else(|| runtime_error(format!("Failed to open {path}")))?;
            // sub_10045A1C8 constructs and validates the decoder before it
            // looks up or replaces the named AudioClip map node. Preserve
            // that transaction boundary: a failed decoder must leave the old
            // named pointer and every instance retaining it untouched.
            let file_info =
                legacy_usage::audio_file_info(&resolved, _streaming).ok_or_else(|| {
                    runtime_error(format!(
                        "Unsupported audio file format while reading {}",
                        resolved.display()
                    ))
                })?;
            let asset = AudioAssetState {
                source: file_info.source,
                duration: file_info.duration,
                sample_frames: file_info.sample_frames,
            };
            create_audio_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_clips
                .insert(name.clone());
            let mut audio = create_audio_runtime
                .lock()
                .expect("audio runtime lock poisoned");
            // Purple has one heterogeneous AudioClip* map. A successful
            // standalone replacement therefore also erases our diagnostic
            // composite metadata for the old object; failed construction
            // returns above and deliberately leaves it intact.
            audio.composite_clips.remove(&name);
            audio.replace_asset(name, Some(asset));
            Ok(())
        })?,
    )?;

    let create_composite_audio_resources = Arc::clone(&resource_runtime);
    let create_composite_audio_runtime = Arc::clone(&audio_runtime);
    resource_api.set(
        "createCompositeAudio",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "res.createCompositeAudio")?;
            let parts = native_required_table(&args, 1, "res.createCompositeAudio")?;
            // sub_100447CBC walks the sequence from one until the first nil,
            // retaining only names that resolve to a live AudioClip.
            let available = create_composite_audio_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_clips
                .clone();
            let available_sources = create_composite_audio_runtime
                .lock()
                .expect("audio runtime lock poisoned")
                .assets
                .clone();
            let mut resolved = Vec::new();
            let mut sources = Vec::new();
            let mut duration = Some(Duration::ZERO);
            let mut sample_frames = Some(0_u64);
            let mut index = 1_i64;
            loop {
                let value = parts.raw_get::<Value>(index)?;
                // sub_10052811C is Lua 5.1's `lua_isstring`: numbers are
                // accepted and converted by sub_100529FB4/lua_tolstring,
                // while nil, booleans, tables and every other tag terminate
                // the contiguous clip-name scan immediately.
                let Some(part) = native_lua51_string(&value) else {
                    break;
                };
                if available.contains(&part) {
                    if let Some(source) = available_sources.get(&part) {
                        sources.push(source.source.clone());
                        duration = duration
                            .zip(source.duration)
                            .and_then(|(total, part)| total.checked_add(part));
                        sample_frames = sample_frames
                            .zip(source.sample_frames)
                            .and_then(|(total, part)| total.checked_add(part));
                    }
                    resolved.push(part);
                }
                index += 1;
            }
            create_composite_audio_resources
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_clips
                .insert(name.clone());
            let mut audio = create_composite_audio_runtime
                .lock()
                .expect("audio runtime lock poisoned");
            audio
                .composite_clips
                .insert(name.clone(), CompositeAudioState { parts: resolved });
            audio.replace_asset(
                name,
                Some(AudioAssetState {
                    source: AudioAssetSource::Sequence(sources),
                    duration,
                    sample_frames,
                }),
            );
            Ok(())
        })?,
    )?;
    Ok(())
}
