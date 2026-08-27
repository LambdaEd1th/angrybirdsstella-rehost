//! Fixed-step GameLua update/draw and immediate capture consumption.

use super::*;

// -[AppController update] clamps the float32 monotonic-clock delta to 0.1f
// before forwarding it to App/GameLua.
const MAX_FRAME_DELTA: Duration = Duration::from_millis(100);

/// Coalesce host wakeups into one display-link callback.
///
/// Purple's native frame entry receives the elapsed display-link delta once;
/// its own GameLua/Box2D layer then accumulates that delta into 30 Hz physics
/// steps. Replaying a full Lua update and draw for every missed desktop tick
/// duplicates the expensive scene traversal and creates a catch-up spiral on
/// object-heavy levels.
fn take_display_link_delta(accumulator: &mut Duration, elapsed: Duration) -> Option<Duration> {
    *accumulator = accumulator.saturating_add(elapsed).min(MAX_FRAME_DELTA);
    if *accumulator < DISPLAY_LINK_STEP {
        return None;
    }
    Some(std::mem::take(accumulator))
}

/// Preserve the independent CADisplayLink cadence across unrelated input
/// wakeups. `last_tick` is the instant already accounted for by `advance` and
/// `accumulator` is the sub-frame time retained from those wakeups.
pub(super) fn next_display_link_deadline(last_tick: Instant, accumulator: Duration) -> Instant {
    last_tick
        .checked_add(DISPLAY_LINK_STEP.saturating_sub(accumulator))
        .unwrap_or(last_tick)
}

impl StellaApp {
    pub(super) fn advance(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let Some(frame_delta) = take_display_link_delta(&mut self.accumulator, elapsed) else {
            return;
        };

        let result = self
            .runtime
            .update(frame_delta.as_secs_f64())
            .and_then(|_| self.runtime.draw());
        if let Err(error) = result {
            self.fatal_error = Some(error.to_string());
            return;
        }
        if let Err(error) = self.synchronize_sprite_catalog() {
            self.fatal_error = Some(error.to_string());
            return;
        }
        self.assets
            .apply_composite_updates(self.runtime.take_composite_updates());
        self.runtime.swap_frame_commands(
            &mut self.render_commands,
            &mut self.text_commands,
            &mut self.rect_commands,
            &mut self.capture_commands,
        );
        self.screenshot_share_requests
            .extend(self.runtime.take_screenshot_share_requests());
        self.dispatch_platform_actions();
        self.background_color = self.runtime.background_color();
        let audio_state = self.runtime.audio_output_state();
        let finished = if let Some(audio) = self.audio.as_mut() {
            audio.synchronize(audio_state)
        } else {
            self.audio_clock.synchronize(&audio_state, frame_delta)
        };
        self.runtime.finish_audio_playbacks(&finished);
        if !self.capture_commands.is_empty()
            && let Some(renderer) = self.renderer.as_mut()
        {
            let result = self
                .assets
                .prepare_gpu_frame_at_resolution(
                    self.resolution,
                    &self.render_commands,
                    &self.text_commands,
                    &self.rect_commands,
                    &self.capture_commands,
                )
                .and_then(|frame| {
                    renderer.render_offscreen(&self.assets, &frame, self.background_color)
                });
            self.capture_commands.clear();
            if let Err(error) = result {
                self.fatal_error = Some(error.to_string());
            }
        }
    }

    pub(super) fn render(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        let size = window.inner_size();
        let frame = self.assets.prepare_gpu_frame_at_resolution(
            self.resolution,
            &self.render_commands,
            &self.text_commands,
            &self.rect_commands,
            &self.capture_commands,
        )?;
        renderer.render_to_window(
            &self.assets,
            &frame,
            self.background_color,
            size.width,
            size.height,
        )?;
        if !self.screenshot_share_requests.is_empty() {
            let rgba = renderer.read_game_rgba()?;
            let requests = std::mem::take(&mut self.screenshot_share_requests);
            super::sharing::stage_screenshot_shares(&requests, &rgba, self.resolution)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_link_delta_waits_for_a_tick_then_consumes_the_whole_debt_once() {
        let mut accumulator = Duration::ZERO;

        assert_eq!(
            take_display_link_delta(&mut accumulator, Duration::from_millis(8)),
            None
        );
        assert_eq!(accumulator, Duration::from_millis(8));
        assert_eq!(
            take_display_link_delta(&mut accumulator, Duration::from_millis(42)),
            Some(Duration::from_millis(50))
        );
        assert_eq!(accumulator, Duration::ZERO);
    }

    #[test]
    fn display_link_delta_clamps_a_stall_without_leaving_catch_up_debt() {
        let mut accumulator = Duration::from_millis(10);

        assert_eq!(
            take_display_link_delta(&mut accumulator, Duration::from_secs(2)),
            Some(MAX_FRAME_DELTA)
        );
        assert_eq!(accumulator, Duration::ZERO);
    }

    #[test]
    fn display_link_deadline_keeps_early_input_wakeups_on_the_same_tick() {
        let last_tick = Instant::now();
        let retained = Duration::from_millis(8);

        assert_eq!(
            next_display_link_deadline(last_tick, retained),
            last_tick + DISPLAY_LINK_STEP.saturating_sub(retained)
        );
        assert_eq!(
            next_display_link_deadline(last_tick, Duration::ZERO),
            last_tick + DISPLAY_LINK_STEP
        );
    }
}
