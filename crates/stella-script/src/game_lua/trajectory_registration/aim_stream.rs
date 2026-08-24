//! Global Catmull-Rom AimStream at GameLua `+0x668`.

mod control;
mod draw;

pub(super) use control::{install_clear, install_populate, install_time};
pub(super) use draw::install_draw;
