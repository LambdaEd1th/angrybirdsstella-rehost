//! Platform-owned account views. Their pixels and credentials never enter
//! the Lua game target, capture resources, or game label cache.

#[path = "account_ui/editor.rs"]
mod editor;
#[path = "account_ui/keyboard.rs"]
pub(crate) mod keyboard;
#[path = "account_ui/layout.rs"]
mod layout;
#[path = "account_ui/render.rs"]
mod render;
#[path = "account_ui/state.rs"]
mod state;
#[path = "account_ui/strings.rs"]
mod strings;

pub(crate) use render::AccountPainter;
pub(crate) use state::{AccountUi, Command, Field};
