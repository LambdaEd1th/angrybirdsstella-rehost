//! Native `AnimationWrapper` subsystem recovered from Purple.

mod model;
mod registration;
mod transform;

pub(crate) use model::*;
#[cfg(test)]
pub(crate) use registration::REGISTERED_ANIMATION_METHODS;
pub(crate) use registration::{RegistrationContext, install};
pub(crate) use transform::*;
