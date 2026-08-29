//! Stable render-command ABI shared by the Lua runtime and the wgpu frontend.
//!
//! Purple keeps sprite, text/font, and color-mesh submission behind distinct
//! native renderer objects. The Rust facade mirrors those ownership boundaries
//! while preserving the original flat public API used by the host crates.

mod font_shaper;
mod geometry;
mod platform_action;
mod screenshot;
mod sprite;
mod system_font;
mod system_font_layout;
mod text;

pub use geometry::*;
pub use platform_action::*;
pub use screenshot::*;
pub use sprite::*;
pub use system_font::*;
pub use system_font_layout::SystemFontFallbackCatalog;
pub use text::*;
