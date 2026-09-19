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

/// GL_Context scissor `0x10059971C` uses signed 32-bit subtraction before GL
/// intersects with the framebuffer. Clamping edges first incorrectly turns
/// wrapped negative widths/heights into visible (possibly full-screen) clips.
pub(in crate::gpu) fn native_scissor(
    edges: Option<[i32; 4]>,
    resolution: GameResolution,
) -> [u32; 4] {
    let [left, top, right, bottom] = edges.unwrap_or([-32000, -32000, 32000, 32000]);
    let width = i64::from(right.wrapping_sub(left).max(0));
    let height = i64::from(bottom.wrapping_sub(top).max(0));
    let x = i64::from(left);
    let y = i64::from((resolution.height as i32).wrapping_sub(bottom));
    let x0 = x.clamp(0, i64::from(resolution.width));
    let x1 = (x + width).clamp(0, i64::from(resolution.width));
    let y0 = y.clamp(0, i64::from(resolution.height));
    let y1 = (y + height).clamp(0, i64::from(resolution.height));
    [
        x0 as u32,
        resolution.height - y1 as u32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    ]
}

impl PreparedFrame {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::gpu) fn push_mesh(
        &mut self,
        positions: &[[f32; 2]],
        uv: &[[f32; 2]],
        source: &[[f32; 2]],
        uniform: DrawUniform,
        base_texture: String,
        fill_texture: String,
        program: NativeProgram,
    ) {
        debug_assert_eq!(positions.len(), uv.len());
        debug_assert_eq!(positions.len(), source.len());
        if positions.is_empty() {
            return;
        }
        let native_clip = native_scissor(self.current_clip, self.resolution);
        if native_clip[2] == 0 || native_clip[3] == 0 {
            return;
        }
        let scissor = (self.current_clip.is_some()
            || native_clip != [0, 0, self.resolution.width, self.resolution.height])
        .then_some(native_clip);
        let draw_index = self.uniforms.len() as u32;
        self.uniforms.push(uniform);
        let first_vertex = self.vertices.len() as u32;
        self.vertices
            .extend(
                positions
                    .iter()
                    .zip(uv)
                    .zip(source)
                    .map(|((position, uv), source)| GpuVertex {
                        position: *position,
                        uv: *uv,
                        source: *source,
                        clip_position: {
                            let [x, y] = if self.current_raw_vertices {
                                *position
                            } else {
                                screen_to_clip(*position, self.resolution)
                            };
                            self.current_projection
                                .map_or([x, y, 0.0, 1.0], |projection| {
                                    let [x, y, z, w] = native_project_clip(
                                        projection,
                                        [x, y, self.current_vertex_depth],
                                    );
                                    // OpenGL clips z to [-w,+w]; wgpu uses [0,w].
                                    [x, y, (z + w) * 0.5, w]
                                })
                        },
                        draw_index,
                        padding: 0,
                    }),
            );
        let vertex_count = self.vertices.len() as u32 - first_vertex;
        // Purple's GL context keeps adjacent submissions with the same
        // program, texture pair and clip in one vertex batch.  Each vertex
        // already carries its own storage-uniform index, so wgpu can retain
        // the exact immediate-mode painter order while issuing one draw for
        // the whole compatible run.  This is especially important for the
        // shipped 16-pixel sky strips: ThemeManager legitimately emits a few
        // hundred adjacent columns, but they are not a few hundred native GL
        // draw calls.
        let previous_draw_index = self.operations.last().and_then(|operation| {
            let PreparedOperation::Draw(index) = operation else {
                return None;
            };
            Some(*index)
        });
        if let Some(index) = previous_draw_index {
            let previous = &self.draws[index];
            let previous_pair = &self.texture_pairs[previous.texture_pair];
            if previous.vertices.end == first_vertex
                && previous_pair.0 == base_texture
                && previous_pair.1 == fill_texture
                && previous.program == program
                && previous.scissor == scissor
            {
                self.draws[index].vertices.end += vertex_count;
                return;
            }
        }

        // Native GL_State retains texture bindings across submissions. Keep
        // one owned copy of each texture pair per prepared frame so the wgpu
        // pass can resolve/cache it once instead of allocating and hashing a
        // fresh `(String, String)` key for every draw batch.
        let texture_pair = self
            .texture_pairs
            .iter()
            .position(|pair| pair.0 == base_texture && pair.1 == fill_texture)
            .unwrap_or_else(|| {
                if base_texture != WHITE_TEXTURE {
                    self.required_textures.insert(base_texture.clone());
                }
                if fill_texture != WHITE_TEXTURE {
                    self.required_textures.insert(fill_texture.clone());
                }
                let index = self.texture_pairs.len();
                self.texture_pairs.push((base_texture, fill_texture));
                index
            });
        let draw_index = self.draws.len();
        self.draws.push(PreparedDraw {
            vertices: first_vertex..first_vertex + vertex_count,
            texture_pair,
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
        uniform: DrawUniform,
        base_texture: String,
        fill_texture: String,
        program: NativeProgram,
    ) {
        let indices = [0usize, 1, 2, 2, 1, 3];
        let triangle_positions = indices.map(|index| positions[index]);
        let triangle_uv = indices.map(|index| uv[index]);
        let triangle_source = indices.map(|index| source[index]);
        self.push_mesh(
            &triangle_positions,
            &triangle_uv,
            &triangle_source,
            uniform,
            base_texture,
            fill_texture,
            program,
        );
    }
}
