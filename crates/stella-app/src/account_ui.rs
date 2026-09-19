//! Platform-owned account views. Their pixels and credentials never enter
//! the Lua game target, capture resources, or game label cache.

mod editor;
mod layout;
mod render;
mod state;
mod strings;

pub(crate) use render::AccountPainter;
pub(crate) use state::{AccountUi, Command, Field};
