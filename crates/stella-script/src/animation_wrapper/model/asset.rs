//! Animation asset loading split along the native wrapper entry boundaries.

mod loading;
mod runtime;
mod skins;

pub(crate) use loading::animation_asset;
pub(crate) use runtime::install_animation_asset;
#[cfg(test)]
pub(crate) use skins::parse_animation_skins;
