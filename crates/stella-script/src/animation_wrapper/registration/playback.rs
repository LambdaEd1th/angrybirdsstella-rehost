//! AnimationWrapper playback bindings, grouped by native member responsibility.

mod callback;
mod controls;
mod update;

pub(super) use callback::install_callback;
pub(super) use controls::install_controls_with_resources;
pub(super) use controls::stop_all_native;
pub(super) use update::install_update_with_resources;
