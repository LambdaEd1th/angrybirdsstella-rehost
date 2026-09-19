use crate::*;

pub(crate) fn native_line_command(
    start: (f64, f64),
    end: (f64, f64),
    width: f64,
    color: [f64; 4],
    state: RenderState,
    clip_rect: Option<[i32; 4]>,
) -> Option<RectRenderCommand> {
    let start_local = (start.0 as f32, start.1 as f32);
    let end_local = (end.0 as f32, end.1 as f32);
    let delta_x = end_local.0 - start_local.0;
    let delta_y = end_local.1 - start_local.1;
    let length = delta_x.hypot(delta_y);
    if length <= f32::EPSILON {
        return None;
    }
    let direction_x = delta_x / length;
    let direction_y = delta_y / length;
    // sub_100598890 direction-weights both live state scales, FCVTZS's the
    // result and clamps values <=1 to one physical pixel.
    let weighted_width = width as f32 * state.scale_x as f32 * direction_y * direction_y
        + width as f32 * state.scale_y as f32 * direction_x * direction_x;
    let effective_width = native_fcvtzs_f32(weighted_width).max(1) as f32;
    let start =
        native_state_screen_point(state, f64::from(start_local.0), f64::from(start_local.1))
            .map(|value| value as f32);
    let end = native_state_screen_point(state, f64::from(end_local.0), f64::from(end_local.1))
        .map(|value| value as f32);
    let ndc_delta_x = (end[0] - start[0]) * 2.0_f32 / 1024.0_f32;
    let ndc_delta_y = -(end[1] - start[1]) * 2.0_f32 / 768.0_f32;
    let ndc_length = ndc_delta_x.hypot(ndc_delta_y);
    if ndc_length <= f32::EPSILON {
        return None;
    }
    let ndc_direction_x = ndc_delta_x / ndc_length;
    let ndc_direction_y = ndc_delta_y / ndc_length;
    let normal_x = -ndc_direction_y * effective_width * 0.5_f32;
    let normal_y = -ndc_direction_x * effective_width * 0.5_f32;
    let vertices = vec![
        [
            f64::from(start[0] + normal_x),
            f64::from(start[1] + normal_y),
        ],
        [
            f64::from(start[0] - normal_x),
            f64::from(start[1] - normal_y),
        ],
        [f64::from(end[0] + normal_x), f64::from(end[1] + normal_y)],
        [f64::from(end[0] - normal_x), f64::from(end[1] - normal_y)],
    ];
    let half_width = effective_width * 0.5_f32;
    let color_program = if color[3] as f32 == 1.0 && state.alpha as f32 == 1.0 {
        ColorProgram::Plain
    } else {
        ColorProgram::PlainAlpha
    };
    Some(RectRenderCommand {
        projection_3d: None,
        order: 0,
        red: f64::from(color[0] as f32),
        green: f64::from(color[1] as f32),
        blue: f64::from(color[2] as f32),
        alpha: f64::from((color[3] as f32) * (state.alpha as f32)),
        left: f64::from(start[0].min(end[0]) - half_width),
        top: f64::from(start[1].min(end[1]) - half_width),
        right: f64::from(start[0].max(end[0]) + half_width),
        bottom: f64::from(start[1].max(end[1]) + half_width),
        color_program,
        vertices: Some(vertices),
        mesh_topology: ColorMeshTopology::TriangleStrip,
        clip_rect,
    })
}
