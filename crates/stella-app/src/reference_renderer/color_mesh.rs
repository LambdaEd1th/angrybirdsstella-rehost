use super::*;

pub(super) fn draw_rect(command: &RectRenderCommand, target: &mut [u32]) {
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
    {
        return;
    }
    let min_x = command.left.floor().clamp(0.0, GAME_WIDTH as f64) as u32;
    let min_y = command.top.floor().clamp(0.0, GAME_HEIGHT as f64) as u32;
    let max_x = command.right.ceil().clamp(0.0, GAME_WIDTH as f64) as u32;
    let max_y = command.bottom.ceil().clamp(0.0, GAME_HEIGHT as f64) as u32;
    let channel = |value: f64| {
        if value <= 1.0 {
            (value.clamp(0.0, 1.0) * 255.0) as u8
        } else {
            value.clamp(0.0, 255.0) as u8
        }
    };
    let source = [
        channel(command.red),
        channel(command.green),
        channel(command.blue),
        255,
    ];
    let alpha = (command.alpha.clamp(0.0, 1.0) * 255.0) as u32;
    if let Some(vertices) = &command.vertices {
        if vertices.len() < 3 || !vertices.iter().flatten().copied().all(f64::is_finite) {
            return;
        }
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let covered = match command.mesh_topology {
                    ColorMeshTopology::TriangleFan => (1..vertices.len() - 1).any(|index| {
                        point_in_triangle(point, vertices[0], vertices[index], vertices[index + 1])
                    }),
                    ColorMeshTopology::TriangleStrip => (0..vertices.len() - 2).any(|index| {
                        let triangle = if index & 1 == 0 {
                            [vertices[index], vertices[index + 1], vertices[index + 2]]
                        } else {
                            [vertices[index + 1], vertices[index], vertices[index + 2]]
                        };
                        point_in_triangle(point, triangle[0], triangle[1], triangle[2])
                    }),
                    ColorMeshTopology::TriangleList => vertices.chunks_exact(3).any(|triangle| {
                        point_in_triangle(point, triangle[0], triangle[1], triangle[2])
                    }),
                };
                if covered {
                    let index = (y * GAME_WIDTH + x) as usize;
                    target[index] = alpha_blend(target[index], source, alpha);
                }
            }
        }
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let index = (y * GAME_WIDTH + x) as usize;
            target[index] = alpha_blend(target[index], source, alpha);
        }
    }
}

fn point_in_triangle(point: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if denominator.abs() <= f64::EPSILON {
        return false;
    }
    let wa = ((b[1] - c[1]) * (point[0] - c[0]) + (c[0] - b[0]) * (point[1] - c[1])) / denominator;
    let wb = ((c[1] - a[1]) * (point[0] - c[0]) + (a[0] - c[0]) * (point[1] - c[1])) / denominator;
    let wc = 1.0 - wa - wb;
    wa >= -1e-9 && wb >= -1e-9 && wc >= -1e-9
}

pub(super) fn alpha_blend(destination: u32, source: [u8; 4], alpha: u32) -> u32 {
    let inverse = 255 - alpha;
    let destination_r = destination >> 16 & 0xff;
    let destination_g = destination >> 8 & 0xff;
    let destination_b = destination & 0xff;
    let blend = |source: u8, destination: u32| {
        (u32::from(source) * alpha + destination * inverse + 127) / 255
    };
    (blend(source[0], destination_r) << 16)
        | (blend(source[1], destination_g) << 8)
        | blend(source[2], destination_b)
}
