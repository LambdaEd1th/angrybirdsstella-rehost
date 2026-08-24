//! CPU-side primitives shared by recovered GameLua draw adapters.
//!
//! The modules mirror Purple's independent rectangle, polygon, line and
//! direct-sprite native members instead of collecting unrelated draw paths.

mod color;
mod line;
mod polygon;
mod rect;
mod software;
mod sprite;
mod transform;

pub(crate) use color::*;
pub(crate) use line::*;
pub(crate) use polygon::*;
pub(crate) use rect::*;
pub(crate) use software::*;
pub(crate) use sprite::*;
pub(crate) use transform::*;
