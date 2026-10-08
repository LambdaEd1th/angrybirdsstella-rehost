//! Desktop test convenience adapters for the shared production frame builder.

use super::*;

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

    #[cfg(test)]
    pub(crate) fn prepare_gpu_frame_at_resolution(
        &mut self,
        resolution: GameResolution,
        commands: &[RenderCommand],
        text_commands: &[TextRenderCommand],
        rect_commands: &[RectRenderCommand],
        capture_commands: &[CaptureRenderCommand],
    ) -> Result<PreparedFrame> {
        self.prepare_gpu_frame_with_shares_at_resolution(
            resolution,
            commands,
            text_commands,
            rect_commands,
            capture_commands,
            &[],
        )
    }
}
