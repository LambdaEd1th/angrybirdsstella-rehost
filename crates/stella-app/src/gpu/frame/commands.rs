//! Native-order frame traversal, command dispatch and framebuffer capture.

use super::{geometry::append_gpu_rect, *};

impl AssetCatalog {
    #[cfg(test)]
    pub(crate) fn prepare_gpu_frame(
        &mut self,
        commands: &[RenderCommand],
        text_commands: &[TextRenderCommand],
        rect_commands: &[RectRenderCommand],
        capture_commands: &[CaptureRenderCommand],
    ) -> Result<PreparedFrame> {
        self.prepare_gpu_frame_at_resolution(
            GameResolution::default(),
            commands,
            text_commands,
            rect_commands,
            capture_commands,
        )
    }

    pub(crate) fn prepare_gpu_frame_at_resolution(
        &mut self,
        resolution: GameResolution,
        commands: &[RenderCommand],
        text_commands: &[TextRenderCommand],
        rect_commands: &[RectRenderCommand],
        capture_commands: &[CaptureRenderCommand],
    ) -> Result<PreparedFrame> {
        if resolution.width > u32::from(u16::MAX) || resolution.height > u32::from(u16::MAX) {
            return Err(anyhow!(
                "{}x{} exceeds Purple's 16-bit sprite geometry",
                resolution.width,
                resolution.height
            ));
        }
        let capture_width = resolution.width as u16;
        let capture_height = resolution.height as u16;
        for region in self
            .regions
            .values_mut()
            .filter(|region| region.texture.starts_with("<capture:"))
        {
            region.sprite.width = capture_width as i16;
            region.sprite.height = capture_height as i16;
        }

        #[derive(Clone, Copy)]
        enum FrameCommand {
            Rect,
            Sprite,
            Text,
            Capture,
        }

        let mut frame = PreparedFrame {
            resolution,
            ..PreparedFrame::default()
        };
        // RenderBridge allocates one monotonically increasing native draw
        // order before appending to each typed queue. Purple consumes that
        // immediate order directly. Merge the four already-sorted queues in
        // O(n) instead of rebuilding and sorting a combined command vector on
        // every frame. The rank preserves the old deterministic tie order for
        // synthetic tests that manually reuse an order value.
        debug_assert!(
            commands
                .windows(2)
                .all(|pair| pair[0].order <= pair[1].order)
        );
        debug_assert!(
            text_commands
                .windows(2)
                .all(|pair| pair[0].order <= pair[1].order)
        );
        debug_assert!(
            rect_commands
                .windows(2)
                .all(|pair| pair[0].order <= pair[1].order)
        );
        debug_assert!(
            capture_commands
                .windows(2)
                .all(|pair| pair[0].order <= pair[1].order)
        );
        let mut rect_index = 0usize;
        let mut sprite_index = 0usize;
        let mut text_index = 0usize;
        let mut capture_index = 0usize;

        let trace_render = std::env::var_os("STELLA_TRACE_RENDER").is_some();
        if trace_render {
            eprintln!(
                "render commands: {} sprites, {} text, {} geometry, {} captures",
                commands.len(),
                text_commands.len(),
                rect_commands.len(),
                capture_commands.len()
            );
        }
        loop {
            let next = [
                rect_commands
                    .get(rect_index)
                    .map(|command| (command.order, 0_u8, FrameCommand::Rect)),
                commands
                    .get(sprite_index)
                    .map(|command| (command.order, 1_u8, FrameCommand::Sprite)),
                text_commands
                    .get(text_index)
                    .map(|command| (command.order, 2_u8, FrameCommand::Text)),
                capture_commands
                    .get(capture_index)
                    .map(|command| (command.order, 3_u8, FrameCommand::Capture)),
            ]
            .into_iter()
            .flatten()
            .min_by_key(|(order, rank, _)| (*order, *rank));
            let Some((_, _, command)) = next else {
                break;
            };
            match command {
                FrameCommand::Rect => {
                    let command = &rect_commands[rect_index];
                    rect_index += 1;
                    frame.current_clip = command.clip_rect;
                    append_gpu_rect(&mut frame, command);
                }
                FrameCommand::Sprite => {
                    let command = &commands[sprite_index];
                    self.append_gpu_render_command(
                        command,
                        sprite_index,
                        trace_render,
                        &mut frame,
                    )?;
                    sprite_index += 1;
                }
                FrameCommand::Text => {
                    let command = &text_commands[text_index];
                    text_index += 1;
                    frame.current_clip = command.clip_rect;
                    self.append_gpu_text(command, &mut frame)?;
                }
                FrameCommand::Capture => {
                    let command = &capture_commands[capture_index];
                    capture_index += 1;
                    let texture = format!("<capture:{}>", command.name);
                    self.composites.remove(&command.name);
                    self.regions.insert(
                        command.name.clone(),
                        AtlasRegion {
                            texture: texture.clone(),
                            sprite: SpriteRegion {
                                name: command.name.clone(),
                                x: 0,
                                y: 0,
                                width: capture_width as i16,
                                height: capture_height as i16,
                                pivot_x: 0,
                                pivot_y: 0,
                                atlas_rotation: 0,
                            },
                        },
                    );
                    frame.operations.push(PreparedOperation::Capture(texture));
                }
            }
        }
        if std::env::var_os("STELLA_TRACE_GPU_BATCHES").is_some() {
            eprintln!(
                "prepared gpu frame: {} uniforms, {} vertices, {} draws, {} operations, {} textures",
                frame.uniforms.len(),
                frame.vertices.len(),
                frame.draws.len(),
                frame.operations.len(),
                frame.required_textures.len()
            );
        }
        Ok(frame)
    }

    fn append_gpu_render_command(
        &mut self,
        command: &RenderCommand,
        index: usize,
        trace_render: bool,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        let state = command.state;
        frame.current_clip = state.clip_rect;
        if ![
            command.x,
            command.y,
            state.translate_x,
            state.translate_y,
            state.scale_x,
            state.scale_y,
            state.angle,
            state.pivot_x,
            state.pivot_y,
            state.alpha,
        ]
        .into_iter()
        .all(f64::is_finite)
            || state
                .matrix
                .is_some_and(|matrix| !matrix.into_iter().all(f64::is_finite))
            || state
                .native_sprite_quad
                .is_some_and(|quad| !quad.into_iter().flatten().all(f64::is_finite))
        {
            if trace_render {
                eprintln!(
                    "render[{index}] skipped non-finite command for {:?}",
                    command.sprite
                );
            }
            return Ok(());
        }
        if trace_render {
            eprintln!(
                "render[{index}] sprite={:?} texture={:?}@{:.4} dirt={} draw=({:.2},{:.2}) size={:?} state=({:.2},{:.2}; {:.3},{:.3}; angle={:.3}; pivot={:.2},{:.2}; alpha={:.3})",
                command.sprite,
                command.texture,
                command.texture_scale,
                command.dirt.is_some(),
                command.x,
                command.y,
                state.draw_size,
                state.translate_x,
                state.translate_y,
                state.scale_x,
                state.scale_y,
                state.angle,
                state.pivot_x,
                state.pivot_y,
                state.alpha,
            );
        }
        if let Some(positions) = state.native_sprite_quad {
            return self.append_gpu_native_sprite_quad(
                &command.sprite,
                command.bound_region.as_deref(),
                positions,
                state.alpha,
                frame,
            );
        }
        if let Some(quad) = state.explicit_quad {
            return self.append_gpu_explicit_quad(
                &command.sprite,
                command.bound_region.as_deref(),
                quad,
                state.alpha,
                frame,
            );
        }
        let transform = render_command_transform(command);
        if let Some(dirt) = &command.dirt {
            self.append_gpu_dirt(dirt, transform, frame)?;
            return Ok(());
        }
        self.append_gpu_sprite(
            &command.sprite,
            command.bound_region.as_deref(),
            command
                .bound_composite
                .as_ref()
                .map(|parts| parts.as_slice()),
            transform,
            0,
            state.draw_size,
            state.sprite_pivot,
            state.masked_texture_matrix,
            command.texture.as_deref().map(|texture| {
                (
                    texture,
                    command.texture_scale,
                    command.masked_texture_binding.as_ref(),
                )
            }),
            command.shader.as_ref(),
            &command.clip_holes,
            frame,
        )
    }
}
