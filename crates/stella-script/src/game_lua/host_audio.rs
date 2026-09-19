//! GameLua-owned rolling loop maintenance between scene export and Lua update.

use super::StellaLua;
use crate::*;

const ROLLING_AUDIO: [(&str, usize); 3] = [
    ("wood_rolling", 1),
    ("rock_rolling", 0),
    ("light_rolling", 2),
];

impl StellaLua {
    /// Apply the two platform volume-key press edges owned by GameApp's outer
    /// frame, after the native key tables have been published and before
    /// `GameLua::update` observes them.
    ///
    /// `sub_1000293C8` handles `VOLUME_UP` first and `VOLUME_DOWN` second. For
    /// each edge it reads the live AudioOutput master volume (or zero when no
    /// output exists), performs the add and clamp in float32, writes the
    /// output when present, then publishes that same float into
    /// `settings.volume`.
    pub(super) fn apply_native_volume_key_edges(&self) -> Result<(), ScriptError> {
        let pressed = native_lua_object(&self.lua, NativeLuaObject::KeyPressed)?
            .ok_or_else(|| runtime_error("keyPressed is not a table"))?;
        for (key, delta) in [("VOLUME_UP", 0.1_f32), ("VOLUME_DOWN", -0.1_f32)] {
            if !pressed.get::<bool>(key)? {
                continue;
            }

            let volume = {
                let mut resources = self
                    .resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                let current = if resources.audio_output_created {
                    resources.master_volume
                } else {
                    0.0
                };
                let adjusted = current + delta;
                let volume = if adjusted >= 0.0 {
                    adjusted.min(1.0)
                } else {
                    0.0
                };
                if resources.audio_output_created {
                    resources.master_volume = volume;
                }
                volume
            };

            let environment = game_environment(&self.lua)?;
            let settings = environment.get::<mlua::Table>("settings")?;
            settings.set("volume", f64::from(volume))?;
        }
        Ok(())
    }

    /// Return the exact nested Boolean stored at
    /// `settings.root.audioEnabled`. The activation virtual treats absence or
    /// a different type as enabled, while GameLua's frame-head recovery path
    /// requires an explicit `true`, so retain the three-state result here.
    pub(super) fn native_audio_enabled_setting(&self) -> Result<Option<bool>, ScriptError> {
        let environment = game_environment(&self.lua)?;
        let Value::Table(settings) = environment.get::<Value>("settings")? else {
            return Ok(None);
        };
        let Value::Table(root) = settings.get::<Value>("root")? else {
            return Ok(None);
        };
        Ok(match root.get::<Value>("audioEnabled")? {
            Value::Boolean(enabled) => Some(enabled),
            _ => None,
        })
    }

    /// `0x10005E8E4..0x10005EAB4` repairs a stopped AudioOutput before any
    /// input publication or Lua frame callback. Unlike activation, this path
    /// runs only while GameApp+0x520 is set and only for exact Boolean true.
    pub(super) fn recover_native_audio_output(&self) -> Result<(), ScriptError> {
        if !self.application_audio_active.get() {
            return Ok(());
        }
        let needs_recovery = {
            let resources = self
                .resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            resources.audio_output_created && !resources.audio_output_started
        };
        if !needs_recovery || self.native_audio_enabled_setting()? != Some(true) {
            return Ok(());
        }

        // Purple reloads the LuaResources output pointer immediately before
        // calling startAudioOutput, so a replacement/removal during the
        // nested setting lookup cannot start a stale instance.
        let mut resources = self
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        if resources.audio_output_created && !resources.audio_output_started {
            if resources.master_volume == -1.0 {
                resources.master_volume = 1.0;
            }
            resources.audio_output_started = true;
        }
        Ok(())
    }

    pub(super) fn update_native_rolling_audio(&self, levels: [f32; 3]) -> Result<(), ScriptError> {
        let resources = self
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        // The shipped boot creates AudioOutput before the first GameLua frame.
        // Direct host integrations may legally construct a VM without booting;
        // there is no native rolling owner to update in that pre-frame state.
        if !resources.audio_output_created {
            return Ok(());
        }
        let output_started = resources.audio_output_started;
        let available = ROLLING_AUDIO.map(|(name, _)| resources.audio_clips.contains(name));
        drop(resources);

        let mut handles = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .rolling_audio_handles;
        let mut audio = self
            ._audio_runtime
            .lock()
            .expect("audio runtime lock poisoned");
        for (index, &(name, handle_index)) in ROLLING_AUDIO.iter().enumerate() {
            let level = levels[index];
            if level <= 0.0_f32 {
                // LuaResources::stopAudio(resource) marks every instance of
                // the resource. The cached GameLua handle is deliberately not
                // cleared by this branch.
                for clip in audio.clips.values_mut().filter(|clip| clip.name == name) {
                    clip.finished = true;
                }
                continue;
            }

            let already_playing = audio
                .clips
                .values()
                .any(|clip| clip.name == name && !clip.finished);
            if already_playing {
                // AudioManager::setVolume walks both instance vectors and
                // silently ignores a stale integer handle.
                if let Some(clip) = audio
                    .clips
                    .get_mut(&handles[handle_index])
                    .filter(|clip| !clip.finished)
                {
                    clip.volume = level;
                }
            } else if output_started && available[index] {
                handles[handle_index] = audio.play(name.to_owned(), level, true, 2);
            }
        }
        drop(audio);
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .rolling_audio_handles = handles;
        Ok(())
    }
}
