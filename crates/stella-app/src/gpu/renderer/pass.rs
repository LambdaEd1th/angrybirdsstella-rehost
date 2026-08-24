//! Fixed game-target pass splitting, draw submission and framebuffer capture.

use super::super::*;

impl GpuRenderer {
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
            .draws
            .iter()
            .map(|draw| self.texture_bind_group(&draw.base_texture, &draw.fill_texture))
            .collect::<Result<Vec<_>>>()?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella game-target encoder"),
            });
        let clear_color = wgpu::Color {
            r: f64::from(background_color[0]) / 255.0,
            g: f64::from(background_color[1]) / 255.0,
            b: f64::from(background_color[2]) / 255.0,
            a: 1.0,
        };
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
                let load = if target_initialized {
                    wgpu::LoadOp::Load
                } else {
                    wgpu::LoadOp::Clear(clear_color)
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
                    for operation in &frame.operations[first_draw_operation..operation_index] {
                        let PreparedOperation::Draw(draw_index) = operation else {
                            continue;
                        };
                        let draw = &frame.draws[*draw_index];
                        let bind_group = &bind_groups[*draw_index];
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
                        pass.set_bind_group(1, bind_group, &[]);
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
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.game_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &captured.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.resolution.width,
                    height: self.resolution.height,
                    depth_or_array_layers: 1,
                },
            );
            operation_index += 1;
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}
