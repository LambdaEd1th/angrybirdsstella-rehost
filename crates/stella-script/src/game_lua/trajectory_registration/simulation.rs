//! The raw trajectory vector, predictor, and selected-bird member boundaries.

mod access;
mod predictor;
mod selection;

pub(super) use access::{install_clear, install_get};
pub(super) use predictor::install_update;
pub(super) use selection::install_selected;
