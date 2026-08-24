//! Box2D contact velocity-constraint initialization, warm start, and solve.

mod impulses;
mod initialization;
mod model;
mod solve;
mod storage;
mod warm_start;

pub(crate) use model::{ContactBodyState, ContactVelocityBodies, NativeContactVelocityCache};
