//! GameApp activation callbacks adjacent to the platform display-link owner.

use super::{StellaLua, input::NATIVE_FRAME_KEYS};
use crate::*;

impl StellaLua {
    /// Reproduce GameApp's activation virtual at `sub_100029BE8`. Both the
    /// start and stop display-link paths clear the complete platform hold
    /// buffer and native touch vector before GameLua receives its callback.
    /// The press/release edge buffers are intentionally not cleared here.
    pub fn set_application_active(&self, active: bool) -> Result<(), ScriptError> {
        self.set_touches(&[])?;
        let environment = game_environment(&self.lua)?;
        for table_name in ["keyHold", "g_keyHold", "g_keyHoldNotBlocked"] {
            let Value::Table(table) = environment.get::<Value>(table_name)? else {
                continue;
            };
            for key in NATIVE_FRAME_KEYS {
                table.raw_set(key, false)?;
            }
            table.raw_set(1, Value::Nil)?;
        }
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

    /// Reproduce the following GameApp audio-activation virtual at
    /// `sub_100029C24`. An explicit Boolean `settings.root.audioEnabled`
    /// suppresses output restart; an absent or differently typed value keeps
    /// Purple's default-enabled behavior. Audio input is independent.
    pub fn set_application_audio_active(&self, active: bool) -> Result<bool, ScriptError> {
        let environment = game_environment(&self.lua)?;
        let audio_enabled = match environment.get::<Value>("settings")? {
            Value::Table(settings) => match settings.get::<Value>("root")? {
                Value::Table(root) => match root.get::<Value>("audioEnabled")? {
                    Value::Boolean(enabled) => enabled,
                    _ => true,
                },
                _ => true,
            },
            _ => true,
        };

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
