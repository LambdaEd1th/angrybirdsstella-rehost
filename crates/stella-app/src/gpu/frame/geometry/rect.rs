//! `GL_Context::drawRect` colored quad and mesh expansion.

use super::super::*;

pub(in crate::gpu) fn append_gpu_rect(frame: &mut PreparedFrame, command: &RectRenderCommand) {
    if ![
        command.red,
        command.green,
        command.blue,
        command.alpha,
        command.left,
        command.top,
        command.right,
        command.bottom,
    ]
    .into_iter()
    .all(f64::is_finite)
        || command.alpha <= 0.0
        || command.right <= command.left
        || command.bottom <= command.top
    {
        return;
    }
    let channel = |value: f64| {
        if value <= 1.0 {
            value.clamp(0.0, 1.0) as f32
        } else {
            (value.clamp(0.0, 255.0) / 255.0) as f32
        }
    };
    let mut uniform = DrawUniform {
        header: [command.alpha.clamp(0.0, 1.0) as f32, 1.0, 2.0, 0.0],
        diffuse: [
            channel(command.red),
            channel(command.green),
            channel(command.blue),
            1.0,
        ],
        params: [0.0, 1.0, 0.0, 0.0],
        fill: [1.0, 1.0, 0.0, 0.0],
        holes: [[0.0; 4]; MAX_HOLES],
    };
    uniform.header[0] = command.alpha.clamp(0.0, 1.0) as f32;
    if let Some(vertices) = &command.vertices {
        if vertices.len() < 3 || !vertices.iter().flatten().copied().all(f64::is_finite) {
            return;
        }
        let mut positions = Vec::new();
        match command.mesh_topology {
            ColorMeshTopology::TriangleList => {
                if vertices.len() % 3 != 0 {
                    return;
                }
                positions.reserve(vertices.len());
                positions.extend(
                    vertices
                        .iter()
                        .map(|vertex| vertex.map(|value| value as f32)),
                );
            }
            topology => {
                positions.reserve((vertices.len() - 2) * 3);
                for index in 0..vertices.len() - 2 {
                    let triangle = match topology {
                        ColorMeshTopology::TriangleFan => {
                            [vertices[0], vertices[index + 1], vertices[index + 2]]
                        }
                        ColorMeshTopology::TriangleStrip if index & 1 == 0 => {
                            [vertices[index], vertices[index + 1], vertices[index + 2]]
                        }
                        ColorMeshTopology::TriangleStrip => {
                            [vertices[index + 1], vertices[index], vertices[index + 2]]
                        }
                        ColorMeshTopology::TriangleList => unreachable!(),
                    };
                    for vertex in triangle {
                        positions.push(vertex.map(|value| value as f32));
                    }
                }
            }
        }
        let empty = vec![[0.0, 0.0]; positions.len()];
        frame.push_mesh(
            &positions,
            &empty,
            &empty,
            &empty,
            uniform,
            WHITE_TEXTURE.to_owned(),
            WHITE_TEXTURE.to_owned(),
            match command.color_program {
                ColorProgram::Plain => NativeProgram::Plain,
                ColorProgram::PlainAlpha => NativeProgram::PlainAlpha,
            },
        );
        return;
    }
    let positions = [
        [command.left as f32, command.top as f32],
        [command.right as f32, command.top as f32],
        [command.left as f32, command.bottom as f32],
        [command.right as f32, command.bottom as f32],
    ];
    frame.push_quad(
        positions,
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        uniform,
        WHITE_TEXTURE.to_owned(),
        WHITE_TEXTURE.to_owned(),
        match command.color_program {
            ColorProgram::Plain => NativeProgram::Plain,
            ColorProgram::PlainAlpha => NativeProgram::PlainAlpha,
        },
    );
}
