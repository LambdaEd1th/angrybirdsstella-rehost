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

    /// `applicationWillResignActive:` invalidates Purple's display link. A
    /// desktop focus transition does not guarantee a separate touch-cancel
    /// event, so synthesize the aggregate iOS cancellation boundary too.
    pub(super) fn will_resign_active(&mut self) {
        if !self.active {
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
}
