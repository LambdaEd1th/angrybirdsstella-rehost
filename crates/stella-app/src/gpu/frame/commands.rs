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
                    frame.current_projection = command.projection_3d;
                    frame.current_raw_vertices = false;
                    frame.current_vertex_depth = 0.001;
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
                    frame.current_projection = command.projection_3d;
                    frame.current_raw_vertices = false;
                    frame.current_vertex_depth = 0.001;
                    self.append_gpu_text(command, &mut frame)?;
                }
                FrameCommand::Capture => {
                    let command = &capture_commands[capture_index];
                    capture_index += 1;
                    let (texture, retired) = self.prepare_capture_texture(
                        &command.texture_source,
                        resolution,
                        command.temporary,
                    )?;
                    if let Some(retired) = retired {
                        frame.retired_textures.insert(retired);
                    }
                    if command.temporary {
                        frame.retired_textures.insert(texture.source.clone());
                    }
                    frame
                        .capture_formats
                        .insert(texture.source.clone(), texture.surface_format);
                    // ResourceManager already registered the SpriteSheet (or
                    // retained an existing one). Only replace its Image pixels;
                    // geometry, aliases, composite priority and draw bindings
                    // continue to come from the script resource catalog.
                    frame
                        .operations
                        .push(PreparedOperation::Capture(texture.source));
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
        frame.current_projection = command.projection_3d.as_deref().copied();
        let is_composite = command.bound_composite.is_some()
            || (command.bound_region.is_none()
                && self.composites.contains_key(command.sprite.as_str()));
        frame.current_raw_vertices =
            matches!(
                command.geometry,
                Some(SpriteGeometrySubmission::RawAtlasQuad(_))
            ) || (frame.current_projection.is_some_and(|p| p.custom_model)
                && !command.world_space
                && command.geometry.is_none()
                && command.texture.is_none()
                && !is_composite);
        frame.current_vertex_depth =
            if command.world_space || frame.current_raw_vertices || is_composite {
                0.0
            } else {
                0.001
            };
        if ![command.x, command.y].into_iter().all(f32::is_finite)
            || ![
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
            .all(f32::is_finite)
            || state
                .matrix
                .is_some_and(|matrix| !matrix.into_iter().all(f32::is_finite))
            || command.geometry.as_ref().is_some_and(|geometry| {
                let finite = match geometry {
                    SpriteGeometrySubmission::ExplicitQuad(quad) => quad
                        .positions
                        .into_iter()
                        .chain(quad.uv)
                        .flatten()
                        .all(f64::is_finite),
                    SpriteGeometrySubmission::NativeAtlasQuad(quad)
                    | SpriteGeometrySubmission::RawAtlasQuad(quad) => {
                        quad.iter().flatten().copied().all(f64::is_finite)
                    }
                };
                !finite
            })
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
                command.texture_name(),
                command.texture_scale(),
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
        match command.geometry.as_ref() {
            Some(SpriteGeometrySubmission::NativeAtlasQuad(positions))
            | Some(SpriteGeometrySubmission::RawAtlasQuad(positions)) => {
                return self.append_gpu_native_sprite_quad(
                    &command.sprite,
                    command.bound_region.as_deref(),
                    **positions,
                    state.alpha,
                    frame,
                );
            }
            Some(SpriteGeometrySubmission::ExplicitQuad(quad)) => {
                return self.append_gpu_explicit_quad(
                    &command.sprite,
                    command.bound_region.as_deref(),
                    **quad,
                    state.alpha,
                    frame,
                );
            }
            None => {}
        }
        let transform = if frame.current_raw_vertices {
            SpriteTransform::from_scale_rotation(command.x, command.y, 1.0, 1.0, 0.0, state.alpha)
        } else {
            render_command_transform(command)
        };
        if let Some(dirt) = command.dirt.as_deref() {
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
            command.texture_name().map(|texture| {
                (
                    texture,
                    command.texture_scale(),
                    command.masked_texture_binding(),
                )
            }),
            command.shader.as_deref(),
            frame,
        )
    }
}
