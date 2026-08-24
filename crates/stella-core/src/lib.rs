//! Platform-independent contracts for the rehosted game runtime.

pub mod physics;
pub mod services;
pub mod time;

pub use physics::{BodyHandle, PhysicsBackend, PhysicsConfig, Vec2};
pub use services::{NullServices, PlatformServices, ServiceError};
pub use time::FixedClock;

/// Shared runtime settings corresponding to fields read by the original
/// `GameLua` initializer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeConfig {
    pub deterministic_physics: bool,
    pub game_world_scale: f32,
    pub physics_hz: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            deterministic_physics: true,
            game_world_scale: 1.0,
            physics_hz: 60,
        }
    }
}
