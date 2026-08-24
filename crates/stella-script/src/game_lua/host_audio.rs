//! GameLua-owned rolling loop maintenance between scene export and Lua update.

use super::StellaLua;
use crate::*;

const ROLLING_AUDIO: [(&str, usize); 3] = [
    ("wood_rolling", 1),
    ("rock_rolling", 0),
    ("light_rolling", 2),
];

impl StellaLua {
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
                audio.clips.retain(|_, clip| clip.name != name);
                continue;
            }

            let already_playing = audio.clips.values().any(|clip| clip.name == name);
            if already_playing {
                // AudioManager::setVolume walks both instance vectors and
                // silently ignores a stale integer handle.
                if let Some(clip) = audio.clips.get_mut(&handles[handle_index]) {
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
