use super::*;

#[allow(dead_code)]
pub(super) fn render_game(
    assets: &mut AssetCatalog,
    commands: &[RenderCommand],
    text_commands: &[TextRenderCommand],
    rect_commands: &[RectRenderCommand],
    background_color: [u8; 3],
    target: &mut [u32],
) -> Result<()> {
    render_game_with_captures(
        assets,
        commands,
        text_commands,
        rect_commands,
        &[],
        background_color,
        target,
    )
}

#[allow(dead_code)]
pub(super) fn render_game_with_captures(
    assets: &mut AssetCatalog,
    commands: &[RenderCommand],
    text_commands: &[TextRenderCommand],
    rect_commands: &[RectRenderCommand],
    capture_commands: &[CaptureRenderCommand],
    background_color: [u8; 3],
    target: &mut [u32],
) -> Result<()> {
    // The original renderer clears before the background/theme passes. A sky
    // blue clear also keeps uncovered letterbox-safe areas deterministic.
    target.fill(
        (u32::from(background_color[0]) << 16)
            | (u32::from(background_color[1]) << 8)
            | u32::from(background_color[2]),
    );
    if !capture_commands.is_empty()
        || !assets.captures.bindings.is_empty()
        || commands
            .iter()
            .any(|command| command.projection_3d.is_some())
        || text_commands
            .iter()
            .any(|command| command.projection_3d.is_some())
        || rect_commands
            .iter()
            .any(|command| command.projection_3d.is_some())
    {
        // Perspective and captured images require the complete native-order
        // stream: geometry preserves painter order and capture generations are
        // resolved at each draw, including draws from subsequent frames.
        let frame =
            assets.prepare_gpu_frame(commands, text_commands, rect_commands, capture_commands)?;
        return frame.render_reference(assets, target);
    }
    for command in rect_commands {
        super::color_mesh::draw_rect(command, target);
    }
    let trace_render = std::env::var_os("STELLA_TRACE_RENDER").is_some();
    if trace_render {
        eprintln!("render commands: {}", commands.len());
    }
    for (index, command) in commands.iter().enumerate() {
        let state = command.state;
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
        {
            if trace_render {
                eprintln!(
                    "render[{index}] skipped non-finite command for {:?}",
                    command.sprite
                );
            }
            continue;
        }
        if trace_render {
            eprintln!(
                "render[{index}] sprite={:?} texture={:?}@{:.4} draw=({:.2},{:.2}) size={:?} state=({:.2},{:.2}; {:.3},{:.3}; angle={:.3}; pivot={:.2},{:.2}; alpha={:.3})",
                command.sprite,
                command.texture_name(),
                command.texture_scale(),
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
        if let Some(SpriteGeometrySubmission::ExplicitQuad(quad)) = command.geometry.as_ref() {
            assets.draw_explicit_quad(
                &command.sprite,
                command.bound_region.as_deref(),
                **quad,
                state.alpha,
                state.clip_rect,
                target,
            )?;
            continue;
        }
        let transform = render_command_transform(command);
        assets.draw_sprite(
            &command.sprite,
            // `setRenderState` stores translation in pre-scale space. The
            // original UI deliberately passes `screen_x / scale_x` (and the
            // corresponding y value), so the native matrix scales both the
            // draw offset and the stored translation.
            transform,
            target,
            0,
            SpriteDrawOptions {
                masked_texture: command
                    .texture_name()
                    .map(|texture| (texture, command.texture_scale())),
                masked_texture_matrix: state.masked_texture_matrix,
                shader: command.shader.as_deref(),
                draw_size: state.draw_size,
                sprite_pivot: state.sprite_pivot,
            },
        )?;
    }
    for command in text_commands {
        assets.draw_text(command, target)?;
    }
    Ok(())
}
