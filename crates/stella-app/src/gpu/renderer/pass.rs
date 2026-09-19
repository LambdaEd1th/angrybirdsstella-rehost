//! Fixed game-target pass splitting, draw submission and framebuffer capture.

use super::super::*;

impl GpuRenderer {
    pub(crate) fn render_offscreen_with_clip(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: [u8; 3],
        clip: Option<[i32; 4]>,
    ) -> Result<()> {
        let [x, y, width, height] = frame::native_scissor(clip, self.resolution);
        if [x, y, width, height] == [0, 0, self.resolution.width, self.resolution.height] {
            return self.render_game(assets, frame, background_color);
        }
        if width != 0 && height != 0 {
            // wgpu attachment clears ignore scissor. Represent GL's partial
            // clear by an opaque, unprojected overwrite, independent of the
            // Lua program/model/alpha state, before loading the draw stream.
            let mut clear = PreparedFrame {
                resolution: self.resolution,
                ..PreparedFrame::default()
            };
            let [red, green, blue] = background_color.map(|v| f64::from(v) / 255.0);
            frame::append_gpu_rect(
                &mut clear,
                &RectRenderCommand {
                    projection_3d: None,
                    order: 0,
                    red,
                    green,
                    blue,
                    alpha: 1.0,
                    left: f64::from(x),
                    top: f64::from(y),
                    right: f64::from(x + width),
                    bottom: f64::from(y + height),
                    color_program: ColorProgram::Plain,
                    vertices: None,
                    mesh_topology: ColorMeshTopology::TriangleStrip,
                    clip_rect: None,
                },
            );
            self.render_before_clear(assets, &clear)?;
        }
        self.render_before_clear(assets, frame)
    }

    #[cfg(test)]
    pub(crate) fn render_offscreen(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: [u8; 3],
    ) -> Result<()> {
        self.render_game(assets, frame, background_color)
    }

    pub(super) fn render_game(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: [u8; 3],
    ) -> Result<()> {
        self.render_stream(assets, frame, Some(background_color))
    }

    /// Native calls made before App's clear (including Lua update and platform
    /// callbacks) operate on the existing framebuffer without an implicit clear.
    pub(crate) fn render_before_clear(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
    ) -> Result<()> {
        self.render_stream(assets, frame, None)
    }

    fn render_stream(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: Option<[u8; 3]>,
    ) -> Result<()> {
        if frame.resolution != self.resolution {
            return Err(anyhow!(
                "prepared frame is {}x{}, renderer target is {}x{}",
                frame.resolution.width,
                frame.resolution.height,
                self.resolution.width,
                self.resolution.height
            ));
        }
        self.sync_textures(assets, frame)?;
        let fallback_uniforms = [DrawUniform::default()];
        let uniforms = if frame.uniforms.is_empty() {
            &fallback_uniforms[..]
        } else {
            &frame.uniforms
        };
        let uniform_bytes = bytemuck::cast_slice(uniforms);
        let vertex_bytes = bytemuck::cast_slice(&frame.vertices);
        self.ensure_stream_capacity(uniform_bytes.len() as u64, vertex_bytes.len() as u64)?;
        self.queue
            .write_buffer(&self.draw_storage_buffer, 0, uniform_bytes);
        if !vertex_bytes.is_empty() {
            self.queue
                .write_buffer(&self.vertex_buffer, 0, vertex_bytes);
        }
        let bind_groups = frame
            .texture_pairs
            .iter()
            .map(|(base, fill)| self.texture_bind_group(base, fill))
            .collect::<Result<Vec<_>>>()?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella game-target encoder"),
            });
        let clear_color = background_color.map(|color| wgpu::Color {
            r: f64::from(color[0]) / 255.0,
            g: f64::from(color[1]) / 255.0,
            b: f64::from(color[2]) / 255.0,
            a: 1.0,
        });
        let mut operation_index = 0usize;
        let mut target_initialized = false;
        while operation_index < frame.operations.len() || !target_initialized {
            let first_draw_operation = operation_index;
            while matches!(
                frame.operations.get(operation_index),
                Some(PreparedOperation::Draw(_))
            ) {
                operation_index += 1;
            }
            if operation_index > first_draw_operation || !target_initialized {
                let load = match (target_initialized, clear_color) {
                    (false, Some(color)) => wgpu::LoadOp::Clear(color),
                    _ => wgpu::LoadOp::Load,
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Reverse-aligned Purple 2D pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.game_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                if !frame.vertices.is_empty() {
                    pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    pass.set_bind_group(0, &self.draw_storage_bind_group, &[]);
                    let mut current_program = None;
                    let mut current_scissor = None;
                    let mut current_texture_pair = None;
                    for operation in &frame.operations[first_draw_operation..operation_index] {
                        let PreparedOperation::Draw(draw_index) = operation else {
                            continue;
                        };
                        let draw = &frame.draws[*draw_index];
                        if current_program != Some(draw.program) {
                            let pipeline = match draw.program {
                                NativeProgram::Plain => &self.plain_program,
                                NativeProgram::PlainAlpha => &self.plain_alpha_program,
                                NativeProgram::Sprite => &self.sprite_program,
                                NativeProgram::SpriteAlpha => &self.sprite_alpha_program,
                                NativeProgram::SpriteAlphaMasked => {
                                    &self.sprite_alpha_masked_program
                                }
                            };
                            pass.set_pipeline(pipeline);
                            current_program = Some(draw.program);
                        }
                        if current_scissor != Some(draw.scissor) {
                            if let Some([x, y, width, height]) = draw.scissor {
                                pass.set_scissor_rect(x, y, width, height);
                            } else {
                                pass.set_scissor_rect(
                                    0,
                                    0,
                                    self.resolution.width,
                                    self.resolution.height,
                                );
                            }
                            current_scissor = Some(draw.scissor);
                        }
                        if current_texture_pair != Some(draw.texture_pair) {
                            pass.set_bind_group(1, &bind_groups[draw.texture_pair], &[]);
                            current_texture_pair = Some(draw.texture_pair);
                        }
                        pass.draw(draw.vertices.clone(), 0..1);
                    }
                }
                drop(pass);
                target_initialized = true;
            }
            let Some(PreparedOperation::Capture(name)) = frame.operations.get(operation_index)
            else {
                break;
            };
            let captured = self
                .textures
                .get(name)
                .ok_or_else(|| anyhow!("capture texture is missing: {name}"))?;
            if captured.texture.width() != self.resolution.width
                || captured.texture.height() != self.resolution.height
            {
                return Err(anyhow!("Wrong size capture target image: {name}"));
            }
            let mut capture_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Stella bottom-up RGB framebuffer capture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &captured.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            capture_pass.set_pipeline(&self.capture_pipeline);
            capture_pass.set_bind_group(0, &self.capture_bind_group, &[]);
            capture_pass.draw(0..3, 0..1);
            drop(capture_pass);
            operation_index += 1;
        }
        self.queue.submit([encoder.finish()]);
        // Wgpu retains submitted resources until GPU work completes. Images
        // discarded by native capture(... on a released sheet) have no owner
        // beyond this operation; do not accumulate one allocation per call.
        self.retire_unused_textures(frame, true);
        Ok(())
    }
}
