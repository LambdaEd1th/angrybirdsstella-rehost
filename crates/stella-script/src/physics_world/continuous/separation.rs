//! `b2SeparationFunction` facade and `b2TimeOfImpact` entry boundaries.

mod function;
mod toi;

pub(crate) use toi::native_time_of_impact;
