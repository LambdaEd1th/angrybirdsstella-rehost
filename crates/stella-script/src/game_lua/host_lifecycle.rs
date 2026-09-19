//! GameApp activation callbacks adjacent to the platform display-link owner.

use super::StellaLua;
use crate::*;

impl StellaLua {
    /// Reproduce GameApp's activation virtual at `sub_100029BE8`. Both the
    /// start and stop display-link paths clear the complete platform hold
    /// buffer and native touch vector before GameLua receives its callback.
    /// The press/release edge buffers and last frame's published Lua input
    /// tables are intentionally not cleared here.
    pub fn set_application_active(&self, active: bool) -> Result<(), ScriptError> {
        self.set_touches(&[])?;
        self.native_keys
            .lock()
            .expect("native key-buffer lock poisoned")
            .clear_holds();
        let environment = game_environment(&self.lua)?;
        // sub_100401678 mutates only GameApp's platform bytes/vector. It does
        // not republish GameLua's keyHold/touches tables, so gamePaused or
        // gameResumed still sees the preceding frame snapshot; the first
        // resumed update replaces it with the cleared platform state.
        // GameLua::setActive stores the new byte first, but suppresses all
        // platform/Lua resume-pause dispatch while its +0x513 gamelogic-load
        // byte is still zero.
        if !self.gamelogic_loaded.get() {
            return Ok(());
        }
        let callback_name = if active { "gameResumed" } else { "gamePaused" };
        if let Value::Function(callback) = environment.get::<Value>(callback_name)? {
            callback.call::<()>(())?;
        }
        Ok(())
    }

    /// Post the SDK activation event after GameApp's resume callback and
    /// before its audio activation. Native AppController404D14 ->0B1F48 ->
    /// 6827BC uses the delayed scheduler; this does not run SDK consumers now.
    pub fn post_application_resumed(&self) {
        self.social.post_application_resumed();
    }

    /// Reproduce the following GameApp audio-activation virtual at
    /// `sub_100029C24`. An explicit Boolean `settings.root.audioEnabled`
    /// suppresses output restart; an absent or differently typed value keeps
    /// Purple's default-enabled behavior. Audio input is independent.
    pub fn set_application_audio_active(&self, active: bool) -> Result<bool, ScriptError> {
        // sub_100029C24 writes GameApp+0x520 before any Lua-table lookup or
        // device operation. The frame-head recovery branch observes it.
        self.application_audio_active.set(active);
        let audio_enabled = self.native_audio_enabled_setting()?.unwrap_or(true);

        let mut resources = self
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        if active {
            if audio_enabled && resources.audio_output_created {
                if resources.master_volume == -1.0 {
                    resources.master_volume = 1.0;
                }
                resources.audio_output_started = true;
            }
            if resources.audio_input_created {
                resources.audio_input_started = true;
            }
        } else {
            if resources.audio_output_created {
                resources.audio_output_started = false;
            }
            if resources.audio_input_created {
                resources.audio_input_started = false;
            }
        }
        Ok(true)
    }
}
