//! App's native update -> immediate stream -> clear -> draw boundary.

use super::*;

impl StellaApp {
    /// Execute a complete display callback, independently of surface acquisition.
    /// `0x100029820..0x100029878` performs update, clear, draw, then present.
    /// The context begin/end slots are no-ops; capture flushes current drawing
    /// immediately, including calls made before Lua's draw callback.
    pub(in crate::app) fn execute_display_frame(
        &mut self,
        mut renderer: Option<&mut GpuRenderer>,
        frame_delta: Duration,
    ) -> Result<()> {
        synchronize_runtime_audio(
            &self.runtime,
            self.audio.as_mut(),
            &mut self.audio_clock,
            frame_delta,
        );
        self.runtime
            .update(frame_delta.as_secs_f64())
            .map_err(|error| anyhow!(error.to_string()))?;

        // Native update/platform callbacks can draw, clearScreen or capture.
        // They run against the existing target, before App's implicit clear.
        // Draining here also preserves capture pixels when Lua draw releases
        // or replaces the same resource. Do not keep only the capture commands:
        // earlier draws in this phase are part of the captured framebuffer.
        self.flush_pending_render_calls(renderer.as_deref_mut())?;

        // The value read at 0x100029830 is used by clear at 0x100029848.
        // A later setBGColor in Lua draw only affects the following clear.
        self.background_color = self.runtime.background_color();
        self.frame_clear_clip = self.runtime.framebuffer_clip_rect();
        self.runtime
            .draw()
            .map_err(|error| anyhow!(error.to_string()))?;
        synchronize_runtime_audio(
            &self.runtime,
            self.audio.as_mut(),
            &mut self.audio_clock,
            Duration::ZERO,
        );
        self.collect_render_stream()?;
        self.rendered_frame_ready = false;
        if let Some(renderer) = renderer {
            self.submit_render_stream(renderer, Some(self.background_color))?;
            self.rendered_frame_ready = true;
        }
        self.screenshot_share_requests
            .extend(self.runtime.take_screenshot_share_requests());
        Ok(())
    }

    /// Native calls outside a display callback are still immediate. Complete
    /// them before presenting or destroying/replacing the target they address.
    pub(in crate::app) fn flush_pending_render_calls(
        &mut self,
        renderer: Option<&mut GpuRenderer>,
    ) -> Result<()> {
        if self.runtime.has_frame_commands() {
            self.collect_render_stream()?;
            if let Some(renderer) = renderer {
                self.submit_render_stream(renderer, None)?;
                self.rendered_frame_ready = true;
            }
        }
        Ok(())
    }

    pub(in crate::app) fn resize_runtime_target(
        &mut self,
        resolution: GameResolution,
    ) -> Result<()> {
        if resolution == self.resolution {
            return Ok(());
        }
        let mut renderer = self.renderer.take();
        let result = self.flush_pending_render_calls(renderer.as_mut());
        self.renderer = renderer;
        result?;
        if let Some(renderer) = &mut self.renderer {
            renderer.resize_game_target(resolution);
        }
        self.rendered_frame_ready = false;
        self.render_commands.clear();
        self.text_commands.clear();
        self.rect_commands.clear();
        self.capture_commands.clear();
        self.resolution = resolution;
        // Callbacks caused by the resolution notification see the new target;
        // the old-size captures above have already consumed their old pixels.
        self.runtime
            .set_screen_resolution(resolution.width, resolution.height)
            .map(|_| ())
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn collect_render_stream(&mut self) -> Result<()> {
        self.synchronize_sprite_catalog()?;
        self.assets
            .apply_composite_updates(self.runtime.take_composite_updates());
        self.runtime.swap_frame_commands(
            &mut self.render_commands,
            &mut self.text_commands,
            &mut self.rect_commands,
            &mut self.capture_commands,
        );
        Ok(())
    }

    fn submit_render_stream(
        &mut self,
        renderer: &mut GpuRenderer,
        clear: Option<[u8; 3]>,
    ) -> Result<()> {
        let frame = self.assets.prepare_gpu_frame_at_resolution(
            self.resolution,
            &self.render_commands,
            &self.text_commands,
            &self.rect_commands,
            &self.capture_commands,
        )?;
        if let Some(background) = clear {
            renderer.render_offscreen_with_clip(
                &self.assets,
                &frame,
                background,
                self.frame_clear_clip,
            )?;
        } else {
            renderer.render_before_clear(&self.assets, &frame)?;
        }
        self.capture_commands.clear();
        Ok(())
    }
}
