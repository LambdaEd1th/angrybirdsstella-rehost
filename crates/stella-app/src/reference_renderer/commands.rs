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
    // The original renderer clears before the background/theme passes. A sky
    // blue clear also keeps uncovered letterbox-safe areas deterministic.
    target.fill(
        (u32::from(background_color[0]) << 16)
            | (u32::from(background_color[1]) << 8)
            | u32::from(background_color[2]),
    );
    for command in rect_commands {
        super::color_mesh::draw_rect(command, target);
    }
    let trace_render = std::env::var_os("STELLA_TRACE_RENDER").is_some();
    if trace_render {
        eprintln!("render commands: {}", commands.len());
    }
    for (index, command) in commands.iter().enumerate() {
        let state = command.state;
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
                clip_holes: &command.clip_holes,
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
