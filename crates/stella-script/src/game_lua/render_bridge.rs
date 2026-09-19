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

    pub(crate) fn push_render_command(&mut self, mut command: RenderCommand) {
        command.projection_3d = self.projection_3d().map(Arc::new);
        if command.state.clip_rect.is_none() {
            command.state.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.commands.push(command);
    }

    pub(crate) fn extend_render_commands(
        &mut self,
        commands: impl IntoIterator<Item = RenderCommand>,
    ) {
        for command in commands {
            self.push_render_command(command);
        }
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
    ) {
        let order = self.allocate_draw_order();
        self.capture_commands.push(CaptureRenderCommand {
            order,
            name,
            texture_source,
            temporary,
        });
    }
}
