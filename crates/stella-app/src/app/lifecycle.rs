//! Native application activation and display-link lifetime.

use super::*;

impl StellaApp {
    fn synchronize_application_audio(&mut self) {
        let audio_state = self.runtime.audio_output_state();
        let finished = if let Some(audio) = self.audio.as_mut() {
            audio.synchronize(audio_state)
        } else {
            self.audio_clock.synchronize(&audio_state, Duration::ZERO)
        };
        self.runtime.finish_audio_playbacks(&finished);
    }

    /// `applicationDidBecomeActive:` snapshots the current monotonic clock
    /// before recreating the display link. Resetting the fixed-step debt here
    /// gives the desktop host the same no-catch-up resume boundary.
    pub(super) fn did_become_active(&mut self) {
        if self.active {
            return;
        }
        self.last_tick = Instant::now();
        self.accumulator = Duration::ZERO;
        if let Err(error) = self.runtime.set_application_active(true) {
            self.fatal_error = Some(error.to_string());
            return;
        }
        if let Err(error) = self.runtime.set_application_audio_active(true) {
            self.fatal_error = Some(error.to_string());
            return;
        }
        self.synchronize_application_audio();
        self.active = true;
    }

    fn stop_application(&mut self, force: bool) {
        if !force && !self.active {
            return;
        }
        self.active = false;
        self.view_did_disappear();
        if let Err(error) = self.runtime.set_application_active(false) {
            self.fatal_error = Some(error.to_string());
        }
        if let Err(error) = self.runtime.set_application_audio_active(false) {
            self.fatal_error = Some(error.to_string());
        }
        self.synchronize_application_audio();
    }

    /// `applicationWillResignActive:` invalidates Purple's display link. A
    /// desktop focus transition does not guarantee a separate touch-cancel
    /// event, so synthesize the aggregate iOS cancellation boundary too.
    pub(super) fn will_resign_active(&mut self) {
        self.stop_application(false);
    }

    /// `applicationWillTerminate:` calls `stopUpdate` unconditionally before
    /// destroying AppController. Unlike an ordinary duplicate focus-loss
    /// notification, this must still deliver `gamePaused` after an earlier
    /// resign-active callback so the shipped script performs its final
    /// settings/highscores/BI persistence pass.
    pub(super) fn application_will_terminate(&mut self) {
        self.stop_application(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termination_forces_final_pause_after_an_already_inactive_transition() {
        let data_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
        if !data_root.join("scripts/game.lua").is_file() {
            return;
        }
        let mut app = StellaApp::new_with_missing_global_diagnostics(
            data_root,
            GameResolution::default(),
            false,
            None,
        )
        .unwrap();
        app.runtime
            .execute_source("assert(g_showTelepodButtons)")
            .unwrap();
        app.runtime
            .execute_source(
                r#"
                    lifecyclePauseCount = 0
                    lifecycleResumeCount = 0
                    function gamePaused()
                        lifecyclePauseCount = lifecyclePauseCount + 1
                    end
                    function gameResumed()
                        lifecycleResumeCount = lifecycleResumeCount + 1
                    end
                "#,
            )
            .unwrap();

        app.did_become_active();
        app.runtime
            .execute_source("assert(lifecycleResumeCount == 1)")
            .unwrap();
        app.will_resign_active();
        app.will_resign_active();
        app.runtime
            .execute_source("assert(lifecyclePauseCount == 1)")
            .unwrap();

        app.application_will_terminate();
        app.runtime
            .execute_source("assert(lifecyclePauseCount == 2)")
            .unwrap();
        assert!(!app.active);
        assert!(!app.runtime.audio_output_state().started);
    }
}
