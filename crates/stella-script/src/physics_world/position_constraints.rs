//! Position-constraint data and `b2PositionSolverManifold` facade.

mod construction;
mod model;
mod world_manifold;

pub(crate) use model::{PositionBodyState, PositionContactConstraint};
