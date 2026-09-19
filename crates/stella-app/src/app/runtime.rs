//! Fixed-step GameLua update/draw and immediate capture consumption.

use super::*;

mod frame_execution;

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

/// Publish output-worker edges before Lua observes the frame, then reconcile
/// any instances or parameter changes created by that frame with zero elapsed
/// output time. Purple's worker is asynchronous to the display link, so the
/// preceding interval must never be advanced after `GameLua::update`.
pub(super) fn synchronize_runtime_audio(
    runtime: &StellaLua,
    audio: Option<&mut AudioDevice>,
    audio_clock: &mut AudioOutputClock,
    elapsed: Duration,
) {
    let audio_state = runtime.audio_output_state();
    let transitions = if let Some(audio) = audio {
        audio.synchronize(audio_state)
    } else {
        audio_clock.synchronize(&audio_state, elapsed)
    };
    runtime.apply_audio_playback_transitions(&transitions);
}

#[cfg(test)]
fn update_and_draw_runtime(
    runtime: &StellaLua,
    audio: Option<&mut AudioDevice>,
    audio_clock: &mut AudioOutputClock,
    frame_delta: Duration,
) -> Result<[u8; 3], stella_script::ScriptError> {
    let mut audio = audio;
    synchronize_runtime_audio(runtime, audio.as_deref_mut(), audio_clock, frame_delta);
    runtime.update(frame_delta.as_secs_f64())?;
    // App/GameLua reads GameLua+0x238 at 0x100029830, passes that packed
    // colour to the framebuffer clear at 0x100029848, and only then invokes
    // Lua `draw` at 0x100029850. Keep the clear colour from that boundary:
    // shipped draw callbacks call setBGColor, but those writes belong to the
    // following frame's clear rather than the commands being built now.
    let background_color = runtime.background_color();
    runtime.draw()?;
    synchronize_runtime_audio(runtime, audio, audio_clock, Duration::ZERO);
    Ok(background_color)
}

impl StellaApp {
    pub(super) fn advance(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let Some(frame_delta) = take_display_link_delta(&mut self.accumulator, elapsed) else {
            return;
        };

        let mut renderer = self.renderer.take();
        let result = self.execute_display_frame(renderer.as_mut(), frame_delta);
        self.renderer = renderer;
        if let Err(error) = result {
            self.fatal_error = Some(error.to_string());
            return;
        }
        self.dispatch_platform_actions();
        if let Err(error) = self.synchronize_account_ui() {
            self.fatal_error = Some(error.to_string());
        }
    }

    /// Presentation fallback for a newly created/resized target. Normal display
    /// ticks already execute their stream before presentation can be skipped.
    fn render_game_frame_if_needed(&mut self) -> Result<()> {
        if self.rendered_frame_ready {
            return Ok(());
        }
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        let frame = self.assets.prepare_gpu_frame_at_resolution(
            self.resolution,
            &self.render_commands,
            &self.text_commands,
            &self.rect_commands,
            &self.capture_commands,
        )?;
        renderer.render_offscreen_with_clip(
            &self.assets,
            &frame,
            self.background_color,
            self.frame_clear_clip,
        )?;
        self.capture_commands.clear();
        self.rendered_frame_ready = true;
        Ok(())
    }

    pub(super) fn render(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        self.render_game_frame_if_needed()?;
        let mut renderer = self.renderer.take();
        let result = self.flush_pending_render_calls(renderer.as_mut());
        self.renderer = renderer;
        result?;
        self.paint_account_overlay(size.width, size.height)?;
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        renderer.present_to_window(size.width, size.height)?;
        if !self.screenshot_share_requests.is_empty() {
            let rgba = renderer.read_game_rgba()?;
            let requests = std::mem::take(&mut self.screenshot_share_requests);
            let paths = super::sharing::stage_screenshot_shares(&requests, &rgba, self.resolution)?;
            for path in paths {
                if let Err(error) =
                    super::platform_actions::launch_external_target(path.to_string_lossy().as_ref())
                {
                    eprintln!("screenshot share action failed: {error}");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod capture_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn mono_pcm_wav(frames: u32) -> Vec<u8> {
        let data_len = frames * 2;
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&32_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(wav.len() + data_len as usize, 0);
        wav
    }

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

    #[test]
    fn framebuffer_clear_color_is_latched_before_shipped_draw_changes_it() {
        let runtime = StellaLua::new("/tmp").unwrap();
        runtime
            .execute_source(
                r#"
                    frame = 0
                    function update()
                        frame = frame + 1
                    end
                    function draw()
                        if frame == 1 then
                            setBGColor(12, 34, 56)
                        else
                            setBGColor(78, 90, 123)
                        end
                    end
                "#,
            )
            .unwrap();
        let mut clock = AudioOutputClock::default();

        let first = update_and_draw_runtime(&runtime, None, &mut clock, Duration::ZERO).unwrap();
        assert_eq!(first, [0xff; 3]);
        assert_eq!(runtime.background_color(), [12, 34, 56]);

        let second = update_and_draw_runtime(&runtime, None, &mut clock, Duration::ZERO).unwrap();
        assert_eq!(second, [12, 34, 56]);
        assert_eq!(runtime.background_color(), [78, 90, 123]);
    }

    #[test]
    fn preceding_audio_interval_finishes_before_lua_and_removes_on_a_later_block() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("stella-app-audio-order-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        let runtime = StellaLua::new(&root).unwrap();
        runtime
            .execute_source("res.createAudioOutput(1, 16, 16000)")
            .unwrap();
        let block_frames = runtime.audio_output_state().buffer_bytes / 2;
        std::fs::write(root.join("edge.wav"), mono_pcm_wav(block_frames * 7)).unwrap();
        runtime
            .execute_source(
                r#"
                    res.createAudio("edge.wav", "EDGE", false)
                    res.startAudioOutput()
                    setChannelCountLimit(2, 1)
                    first = res.playAudio("EDGE", 1, false, 2)
                    frameCount = 0
                    function update()
                        frameCount = frameCount + 1
                        if frameCount == 1 then
                            sawFinishedBeforeUpdate = not res.isAudioPlaying(first)
                            replacement = res.playAudio("EDGE", 1, false, 2)
                        end
                    end
                    function draw() end
                "#,
            )
            .unwrap();

        let mut clock = AudioOutputClock::default();
        synchronize_runtime_audio(&runtime, None, &mut clock, Duration::ZERO);
        assert!(!runtime.audio_output_state().playbacks[0].finished);

        update_and_draw_runtime(&runtime, None, &mut clock, Duration::from_millis(70)).unwrap();
        runtime
            .execute_source("assert(sawFinishedBeforeUpdate and replacement == 1)")
            .unwrap();
        let retained = runtime.audio_output_state();
        assert_eq!(retained.playbacks.len(), 2);
        assert!(
            retained
                .playbacks
                .iter()
                .any(|clip| clip.handle == 0 && clip.finished)
        );

        update_and_draw_runtime(&runtime, None, &mut clock, Duration::from_millis(70)).unwrap();
        let removed = runtime.audio_output_state();
        assert_eq!(removed.playbacks.len(), 1);
        assert_eq!(removed.playbacks[0].handle, 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
