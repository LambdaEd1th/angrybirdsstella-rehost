//! Position-constraint data and `b2PositionSolverManifold` facade.

mod construction;
mod model;
mod solve_math;
mod world_manifold;

pub(crate) use model::{PositionBodyState, PositionContactConstraint};
pub(crate) use solve_math::{
    native_position_correction, native_position_cross, native_position_effective_inverse_mass,
};
