//! Ordered render-command queue operations owned by the GameLua bridge.

use crate::*;

impl RenderBridge {
    pub(crate) fn projection_3d(&self) -> Option<TextProjection3D> {
        self.perspective_projection
            .then(|| self.state.custom_model.unwrap_or_default())
    }

    pub(crate) fn allocate_draw_order(&mut self) -> u64 {
        let order = self.next_draw_order;
        self.next_draw_order = self.next_draw_order.wrapping_add(1);
        order
    }

    pub(crate) fn push_render_command(&mut self, mut command: RenderCommand) -> LuaResult<()> {
        // AtlasSprite resolves SpriteSheet+0x20 at this immediate boundary.
        // Scene/animation/particle owners borrow the sheet; only the submitted
        // command keeps the Image alive while the GPU host consumes it later.
        if command.dirt.is_none() {
            command.bound_region = command
                .bound_region
                .as_ref()
                .map(|region| region.snapshot_image())
                .transpose()?;
            if let Some(parts) = &command.bound_composite
                && parts.iter().any(|part| part.region.sheet_image.is_some())
            {
                let frozen = parts
                    .iter()
                    .map(|part| {
                        Ok(BoundCompositePart {
                            // CompoSprite::draw at 0x1004376D4 reads an Entry's
                            // AtlasSprite only after its visible byte passes.
                            region: if part.part.visible {
                                part.region.snapshot_image()?
                            } else {
                                Arc::clone(&part.region)
                            },
                            ..part.clone()
                        })
                    })
                    .collect::<LuaResult<Vec<_>>>()?;
                command.bound_composite = Some(Arc::new(frozen));
            }
        }
        command.projection_3d = self.projection_3d().map(Arc::new);
        if command.state.clip_rect.is_none() {
            command.state.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.commands.push(command);
        Ok(())
    }

    pub(crate) fn extend_render_commands(
        &mut self,
        commands: impl IntoIterator<Item = RenderCommand>,
    ) -> LuaResult<()> {
        for command in commands {
            self.push_render_command(command)?;
        }
        Ok(())
    }

    pub(crate) fn push_text_command(&mut self, mut command: TextRenderCommand) {
        command.projection_3d = self.projection_3d();
        if command.clip_rect.is_none() {
            command.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.text_commands.push(command);
    }

    pub(crate) fn push_rect_command(&mut self, mut command: RectRenderCommand) {
        command.projection_3d = self.projection_3d();
        if command.clip_rect.is_none() {
            command.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.rect_commands.push(command);
    }

    /// Submit a rectangle after a native state reset, without inheriting the
    /// caller's previous scissor. Scene submissions use their separately
    /// recovered context; framebuffer clear bypasses projection as well and
    /// therefore does not use this ordinary geometry path.
    pub(crate) fn push_unclipped_rect_command(&mut self, mut command: RectRenderCommand) {
        command.projection_3d = self.projection_3d();
        command.clip_rect = None;
        command.order = self.allocate_draw_order();
        self.rect_commands.push(command);
    }

    pub(crate) fn push_capture_command(
        &mut self,
        name: String,
        texture_source: String,
        temporary: bool,
        decoded_image: Option<Arc<stella_assets::native_image::DecodedNativeImage>>,
        image_owner: Option<Arc<crate::NativeImageOwner>>,
    ) {
        let order = self.allocate_draw_order();
        self.capture_commands.push(CaptureRenderCommand {
            order,
            name,
            texture_source,
            temporary,
            decoded_image,
            image_owner,
        });
    }
}
