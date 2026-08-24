//! Test-only CPU reference renderer for recovered GL/wgpu compatibility.
//!
//! Purple keeps command traversal, colored geometry, sprite submission,
//! texture state, pixel programs and viewport presentation in separate native
//! members. Preserve those boundaries in the software oracle as well.

use super::*;

mod color_mesh;
mod commands;
mod presentation;
mod shader;
mod sprite;
mod texture;

pub(super) use sprite::{draw_explicit_quad, draw_region};

#[cfg(test)]
mod tests;
