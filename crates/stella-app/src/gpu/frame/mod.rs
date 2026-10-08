//! CPU-side expansion of recovered immediate GL commands into ordered wgpu draws.

use super::*;

mod batch;
mod commands;
mod geometry;
mod quads;
mod sprites;
mod text;
mod types;

pub(super) use batch::native_scissor;
pub(super) use geometry::append_gpu_rect;
pub(super) use types::{DrawUniform, GpuVertex, PreparedDraw, PreparedOperation};

impl PreparedFrame {
    // The diagnostics flag is constant while this immediate draw stream is
    // prepared. Resolve it once, retaining the original per-region log order.
    fn trace_gpu_regions(&mut self) -> bool {
        *self
            .gpu_region_trace
            .get_or_insert_with(|| std::env::var_os("STELLA_TRACE_GPU_REGIONS").is_some())
    }

    fn retain_texture(&mut self, texture: &crate::assets::ResolvedTexture) {
        if let Some(lease) = &texture.lease {
            self.texture_leases
                .insert(texture.source.clone(), lease.clone());
        }
    }
}

#[cfg(test)]
pub(super) use batch::screen_to_clip;

#[cfg(test)]
pub(super) use geometry::append_gpu_dirt_triangles;
#[cfg(test)]
pub(super) use geometry::shader_uniform;
