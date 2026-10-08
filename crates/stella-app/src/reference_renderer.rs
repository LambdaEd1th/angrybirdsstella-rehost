//! Test-only CPU reference renderer for recovered GL/wgpu compatibility.
//!
//! Purple keeps command traversal, colored geometry, sprite submission,
//! texture state, pixel programs and viewport presentation in separate native
//! members. Preserve those boundaries in the software oracle as well.

use super::*;

// The CPU draw extensions depend on this desktop oracle. They must not be
// pulled into the browser's shared asset catalog merely because cfg(test)
// is enabled by a complete workspace test build.
#[path = "assets/sprite.rs"]
mod asset_sprite;
#[path = "assets/text.rs"]
mod asset_text;

mod color_mesh;
mod commands;
mod presentation;
mod shader;
mod sprite;
mod texture;

pub(super) use sprite::{draw_explicit_quad, draw_region};

#[cfg(test)]
mod tests;
