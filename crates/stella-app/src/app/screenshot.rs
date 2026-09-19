//! Deterministic headless input script and wgpu screenshot execution.

use super::*;

impl StellaApp {
    pub(crate) fn save_screenshot(
        &mut self,
        destination: &PathBuf,
        frames: u32,
        clicks: &[(u32, f64, f64)],
        drags: &[(u32, u32, f64, f64, f64, f64)],
        evals: &[(u32, String)],
    ) -> Result<()> {
        let mut renderer = GpuRenderer::headless(self.resolution)?;
        for frame in 0..frames {
            for &(click_frame, x, y) in clicks {
                if frame == click_frame {
                    self.runtime
                        .set_cursor(x, y, true)
                        .map_err(|error| anyhow!(error.to_string()))?;
                } else if frame == click_frame.saturating_add(1) {
                    self.runtime
                        .set_cursor(x, y, false)
                        .map_err(|error| anyhow!(error.to_string()))?;
                }
            }
            for &(drag_frame, duration, start_x, start_y, end_x, end_y) in drags {
                let end_frame = drag_frame.saturating_add(duration.max(1));
                if frame >= drag_frame && frame < end_frame {
                    let progress = f64::from(frame - drag_frame) / f64::from(duration.max(1));
                    self.runtime
                        .set_cursor(
                            start_x + (end_x - start_x) * progress,
                            start_y + (end_y - start_y) * progress,
                            true,
                        )
                        .map_err(|error| anyhow!(error.to_string()))?;
                } else if frame == end_frame {
                    self.runtime
                        .set_cursor(end_x, end_y, false)
                        .map_err(|error| anyhow!(error.to_string()))?;
                }
            }
            for (eval_frame, source) in evals {
                if frame == *eval_frame {
                    self.runtime
                        .execute_source(source)
                        .map_err(|error| anyhow!(error.to_string()))?;
                }
            }
            // Use the same complete display boundary as the window host. A
            // later update can capture/draw onto an earlier ordinary frame,
            // even if that earlier frame contained no capture of its own.
            self.execute_display_frame(Some(&mut renderer), DISPLAY_LINK_STEP)?;
            if !self.screenshot_share_requests.is_empty() {
                let screenshot_shares = std::mem::take(&mut self.screenshot_share_requests);
                let rgba = renderer.read_game_rgba()?;
                super::sharing::stage_screenshot_shares(
                    &screenshot_shares,
                    &rgba,
                    self.resolution,
                )?;
            }
        }
        let rgba = if frames > 0 {
            // Never replay a consumed stream after capture rebinding.
            renderer.read_game_rgba()?
        } else {
            self.synchronize_sprite_catalog()?;
            self.assets
                .apply_composite_updates(self.runtime.take_composite_updates());
            self.runtime.swap_frame_commands(
                &mut self.render_commands,
                &mut self.text_commands,
                &mut self.rect_commands,
                &mut self.capture_commands,
            );
            let frame = self.assets.prepare_gpu_frame_at_resolution(
                self.resolution,
                &self.render_commands,
                &self.text_commands,
                &self.rect_commands,
                &self.capture_commands,
            )?;
            renderer.render_to_rgba(&self.assets, &frame, self.background_color)?
        };
        if let Some(parent) = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        image::save_buffer(
            destination,
            &rgba,
            self.resolution.width,
            self.resolution.height,
            image::ColorType::Rgba8,
        )
        .with_context(|| format!("save {}", destination.display()))?;
        Ok(())
    }
}
