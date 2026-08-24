use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BodyHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsConfig {
    pub gravity: Vec2,
    pub world_scale: f32,
    pub velocity_iterations: u32,
    pub position_iterations: u32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: Vec2 { x: 0.0, y: -9.81 },
            world_scale: 1.0,
            velocity_iterations: 8,
            position_iterations: 3,
        }
    }
}

/// Solver-independent surface exposed to Lua/gameplay. The concrete backend is
/// intentionally deferred until the original solver and parameters are
/// conclusively identified in IDA.
pub trait PhysicsBackend {
    fn configure(&mut self, config: PhysicsConfig);
    fn step(&mut self, seconds: f32);
    fn position(&self, body: BodyHandle) -> Option<Vec2>;
    fn set_velocity(&mut self, body: BodyHandle, velocity: Vec2);
    fn apply_impulse(&mut self, body: BodyHandle, impulse: Vec2);
    fn remove_body(&mut self, body: BodyHandle);
}
