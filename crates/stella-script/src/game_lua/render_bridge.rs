//! Ordered render-command queue operations owned by the GameLua bridge.

use crate::*;

impl RenderBridge {
    pub(crate) fn allocate_draw_order(&mut self) -> u64 {
        let order = self.next_draw_order;
        self.next_draw_order = self.next_draw_order.wrapping_add(1);
        order
    }

    pub(crate) fn push_render_command(&mut self, mut command: RenderCommand) {
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
        if command.clip_rect.is_none() {
            command.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.text_commands.push(command);
    }

    pub(crate) fn push_rect_command(&mut self, mut command: RectRenderCommand) {
        if command.clip_rect.is_none() {
            command.clip_rect = self.state.clip_rect;
        }
        command.order = self.allocate_draw_order();
        self.rect_commands.push(command);
    }

    /// Submit a rectangle after a native state reset, without inheriting the
    /// caller's previous scissor. `clearScreen` uses this path because Purple
    /// replaces its complete 0x9c-byte GL context before drawing the clear
    /// rectangle.
    pub(crate) fn push_unclipped_rect_command(&mut self, mut command: RectRenderCommand) {
        command.clip_rect = None;
        command.order = self.allocate_draw_order();
        self.rect_commands.push(command);
    }

    pub(crate) fn push_capture_command(&mut self, name: String) {
        let order = self.allocate_draw_order();
        self.capture_commands
            .push(CaptureRenderCommand { order, name });
    }
}
