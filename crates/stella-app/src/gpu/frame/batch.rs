//! Immediate GL-style mesh batching, scissor resolution and quad expansion.

use super::*;

pub(in crate::gpu) fn screen_to_clip(position: [f32; 2], resolution: GameResolution) -> [f32; 2] {
    // GL_Context emits one float32 division for each viewport scale and then
    // FMADD(coordinate, scale, +/-1) before submitting the vertex.
    let x_scale = 2.0_f32 / resolution.width as f32;
    let y_scale = -2.0_f32 / resolution.height as f32;
    [
        position[0].mul_add(x_scale, -1.0),
        position[1].mul_add(y_scale, 1.0),
    ]
}

impl PreparedFrame {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::gpu) fn push_mesh(
        &mut self,
        positions: &[[f32; 2]],
        uv: &[[f32; 2]],
        source: &[[f32; 2]],
        local: &[[f32; 2]],
        uniform: DrawUniform,
        base_texture: String,
        fill_texture: String,
        program: NativeProgram,
    ) {
        debug_assert_eq!(positions.len(), uv.len());
        debug_assert_eq!(positions.len(), source.len());
        debug_assert_eq!(positions.len(), local.len());
        if positions.is_empty() {
            return;
        }
        let scissor = match self.current_clip {
            Some([left, top, right, bottom]) => {
                let left = left.clamp(0, self.resolution.width as i32);
                let top = top.clamp(0, self.resolution.height as i32);
                let right = right.clamp(0, self.resolution.width as i32);
                let bottom = bottom.clamp(0, self.resolution.height as i32);
                if right <= left || bottom <= top {
                    return;
                }
                Some([
                    left as u32,
                    top as u32,
                    (right - left) as u32,
                    (bottom - top) as u32,
                ])
            }
            None => None,
        };
        let draw_index = self.uniforms.len() as u32;
        self.uniforms.push(uniform);
        let first_vertex = self.vertices.len() as u32;
        self.vertices
            .extend(positions.iter().zip(uv).zip(source).zip(local).map(
                |(((position, uv), source), local)| GpuVertex {
                    position: *position,
                    uv: *uv,
                    source: *source,
                    local: *local,
                    clip_position: screen_to_clip(*position, self.resolution),
                    draw_index,
                    padding: 0,
                },
            ));
        let vertex_count = self.vertices.len() as u32 - first_vertex;
        if base_texture != WHITE_TEXTURE {
            self.required_textures.insert(base_texture.clone());
        }
        if fill_texture != WHITE_TEXTURE {
            self.required_textures.insert(fill_texture.clone());
        }
        // Purple's GL context keeps adjacent submissions with the same
        // program, texture pair and clip in one vertex batch.  Each vertex
        // already carries its own storage-uniform index, so wgpu can retain
        // the exact immediate-mode painter order while issuing one draw for
        // the whole compatible run.  This is especially important for the
        // shipped 16-pixel sky strips: ThemeManager legitimately emits a few
        // hundred adjacent columns, but they are not a few hundred native GL
        // draw calls.
        let previous_draw = self.operations.last().and_then(|operation| {
            let PreparedOperation::Draw(index) = operation else {
                return None;
            };
            self.draws.get_mut(*index)
        });
        if let Some(previous) = previous_draw
            && previous.vertices.end == first_vertex
            && previous.base_texture == base_texture
            && previous.fill_texture == fill_texture
            && previous.program == program
            && previous.scissor == scissor
        {
            previous.vertices.end += vertex_count;
            return;
        }

        let draw_index = self.draws.len();
        self.draws.push(PreparedDraw {
            vertices: first_vertex..first_vertex + vertex_count,
            base_texture,
            fill_texture,
            program,
            scissor,
        });
        self.operations.push(PreparedOperation::Draw(draw_index));
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::gpu) fn push_quad(
        &mut self,
        positions: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        source: [[f32; 2]; 4],
        local: [[f32; 2]; 4],
        uniform: DrawUniform,
        base_texture: String,
        fill_texture: String,
        program: NativeProgram,
    ) {
        let indices = [0usize, 1, 2, 2, 1, 3];
        let triangle_positions = indices.map(|index| positions[index]);
        let triangle_uv = indices.map(|index| uv[index]);
        let triangle_source = indices.map(|index| source[index]);
        let triangle_local = indices.map(|index| local[index]);
        self.push_mesh(
            &triangle_positions,
            &triangle_uv,
            &triangle_source,
            &triangle_local,
            uniform,
            base_texture,
            fill_texture,
            program,
        );
    }
}
