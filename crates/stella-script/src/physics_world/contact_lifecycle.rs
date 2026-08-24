//! Contact/joint destruction callbacks and collision-force dispatch.
//!
//! The native implementation separates Box2D joint/contact teardown from
//! GameLua sensor bookkeeping and post-step collision-force processing. Keep
//! those reverse-engineered boundaries visible in the Rust layout as well.

mod contacts;
mod forces;
mod joints;
mod removals;
mod sensors;
