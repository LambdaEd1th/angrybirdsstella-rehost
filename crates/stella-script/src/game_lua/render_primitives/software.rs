use crate::*;

pub(crate) fn push_software_line(
    commands: &mut Vec<RectRenderCommand>,
    start: (f64, f64),
    end: (f64, f64),
    width: f64,
    color: [f64; 4],
    clip_rect: Option<[i32; 4]>,
) {
    let color = [color[0], color[1], color[2], native_color_alpha(color[3])];
    if let Some(command) =
        native_line_command(start, end, width, color, RenderState::default(), clip_rect)
    {
        commands.push(command);
    }
}
