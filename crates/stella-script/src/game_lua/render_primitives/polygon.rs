use crate::*;

pub(crate) fn native_polygon_commands(
    points: &[(f64, f64)],
    offset_x: f64,
    offset_y: f64,
    color: [f64; 4],
    state: RenderState,
) -> Vec<RectRenderCommand> {
    native_polygon_commands_inner(points, offset_x, offset_y, color, state, true)
}

pub(crate) fn native_filled_polygon_commands(
    points: &[(f64, f64)],
    offset_x: f64,
    offset_y: f64,
    color: [f64; 4],
    state: RenderState,
) -> Vec<RectRenderCommand> {
    native_polygon_commands_inner(points, offset_x, offset_y, color, state, false)
}

fn native_polygon_commands_inner(
    points: &[(f64, f64)],
    offset_x: f64,
    offset_y: f64,
    color: [f64; 4],
    state: RenderState,
    outline: bool,
) -> Vec<RectRenderCommand> {
    if points.len() < 3 {
        return Vec::new();
    }
    let points = points
        .iter()
        .map(|&(x, y)| (f64::from(x as f32), f64::from(y as f32)))
        .collect::<Vec<_>>();
    let offset_x = f64::from(offset_x as f32);
    let offset_y = f64::from(offset_y as f32);
    let color = color.map(|channel| f64::from(channel as f32));
    let mut commands = Vec::with_capacity(points.len() + 1);

    let vertices = native_triangulate_polygon(&points)
        .into_iter()
        .flatten()
        .map(|(x, y)| {
            native_polygon_screen_point(state, offset_x, offset_y, f64::from(x), f64::from(y))
        })
        .collect::<Vec<_>>();
    if !vertices.is_empty() {
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
        commands.push(RectRenderCommand {
            projection_3d: None,
            order: 0,
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: f64::from((color[3] as f32) * (state.alpha as f32)),
            left: min_x,
            top: min_y,
            right: max_x,
            bottom: max_y,
            color_program: if color[3] as f32 == 1.0 && state.alpha as f32 == 1.0 {
                ColorProgram::Plain
            } else {
                ColorProgram::PlainAlpha
            },
            vertices: Some(vertices),
            mesh_topology: ColorMeshTopology::TriangleList,
            clip_rect: state.clip_rect,
        });
    }

    if !outline {
        return commands;
    }
    for index in 0..points.len() {
        let start = native_polygon_line_point(points[index], offset_x, offset_y);
        let end = native_polygon_line_point(points[(index + 1) % points.len()], offset_x, offset_y);
        if let Some(command) = native_line_command(
            start,
            end,
            1.0,
            [0.0, 0.0, 0.0, 1.0],
            state,
            state.clip_rect,
        ) {
            commands.push(command);
        }
    }
    commands
}

fn native_polygon_line_point(point: (f64, f64), offset_x: f64, offset_y: f64) -> (f64, f64) {
    let x = native_fcvtzs_f32(((point.0 as f32) + offset_x as f32) * 20.0_f32);
    let y = native_fcvtzs_f32(((point.1 as f32) + offset_y as f32) * 20.0_f32);
    (f64::from(x), f64::from(y))
}

fn native_polygon_screen_point(
    state: RenderState,
    offset_x: f64,
    offset_y: f64,
    point_x: f64,
    point_y: f64,
) -> [f64; 2] {
    let local_x = (point_x as f32) * 20.0_f32;
    let local_y = (point_y as f32) * 20.0_f32;
    // sub_100024A50 adds DrawablePolygon's position outside the context
    // linear basis, while the vertex itself passes through that basis.
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    let base_x = ((state.translate_x as f32) + (offset_x as f32) * 20.0_f32) * scale_x;
    let base_y = ((state.translate_y as f32) + (offset_y as f32) * 20.0_f32) * scale_y;
    if let Some([m00, m01, m10, m11]) = state.matrix {
        return [
            f64::from(base_x + m00 as f32 * local_x + m01 as f32 * local_y),
            f64::from(base_y + m10 as f32 * local_x + m11 as f32 * local_y),
        ];
    }
    let (sine, cosine) = (state.angle as f32).sin_cos();
    let pivot_x = state.pivot_x as f32;
    let pivot_y = state.pivot_y as f32;
    let pivot_correction_x = pivot_x - cosine * pivot_x + sine * pivot_y;
    let pivot_correction_y = pivot_y - sine * pivot_x - cosine * pivot_y;
    [
        f64::from(
            base_x + scale_x * pivot_correction_x + scale_x * cosine * local_x
                - scale_x * sine * local_y,
        ),
        f64::from(
            base_y
                + scale_y * pivot_correction_y
                + scale_y * sine * local_x
                + scale_y * cosine * local_y,
        ),
    ]
}
