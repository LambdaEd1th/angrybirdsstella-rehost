use crate::*;

pub(crate) fn native_rect_command(
    color: [f64; 4],
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    state: RenderState,
) -> RectRenderCommand {
    // sub_100043C14 multiplies normalized RGBA by 255 and packs only the low
    // byte of each FCVTZS result. Every geometry conversion is independent.
    let color = color.map(|channel| {
        let packed = (channel as f32) * 255.0_f32;
        native_packed_color_channel(f64::from(packed))
    });
    let [left, top, right, bottom] = [left, top, right, bottom].map(|value| value as f32);
    let x = f64::from(native_fcvtzs_f32(left));
    let y = f64::from(native_fcvtzs_f32(top));
    let width = f64::from(native_fcvtzs_f32(right - left));
    let height = f64::from(native_fcvtzs_f32(bottom - top));
    let vertices = vec![
        native_state_screen_point(state, x, y),
        native_state_screen_point(state, x + width, y),
        native_state_screen_point(state, x, y + height),
        native_state_screen_point(state, x + width, y + height),
    ];
    let min_x = vertices
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let min_y = vertices
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let max_x = vertices
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = vertices
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let color_program = if color[3] as f32 == 1.0 && state.alpha as f32 == 1.0 {
        ColorProgram::Plain
    } else {
        ColorProgram::PlainAlpha
    };
    RectRenderCommand {
        order: 0,
        red: color[0],
        green: color[1],
        blue: color[2],
        alpha: f64::from((color[3] as f32) * (state.alpha as f32)),
        left: min_x,
        top: min_y,
        right: max_x,
        bottom: max_y,
        color_program,
        vertices: Some(vertices),
        mesh_topology: ColorMeshTopology::TriangleStrip,
        clip_rect: state.clip_rect,
    }
}
