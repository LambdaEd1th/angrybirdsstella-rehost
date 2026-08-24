//! Native atlas, colored-geometry, Dirt and shader-uniform expansion facade.

mod dirt;
mod rect;
mod region;
mod shader;

pub(in crate::gpu) use dirt::append_gpu_dirt_triangles;
pub(super) use rect::append_gpu_rect;
pub(super) use region::append_gpu_region;
pub(in crate::gpu) use shader::shader_uniform;
