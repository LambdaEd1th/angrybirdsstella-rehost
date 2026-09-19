//! CPU-side expansion of recovered immediate GL commands into ordered wgpu draws.

use super::*;

mod batch;
mod commands;
mod geometry;
mod quads;
mod sprites;
mod text;

pub(super) use batch::native_scissor;
pub(super) use geometry::append_gpu_rect;

#[cfg(test)]
pub(super) use batch::screen_to_clip;

#[cfg(test)]
pub(super) use geometry::append_gpu_dirt_triangles;
#[cfg(test)]
pub(super) use geometry::shader_uniform;
