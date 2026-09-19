//! Animation asset loading split along the native wrapper entry boundaries.

mod loading;
mod runtime;
mod skins;

#[cfg(test)]
pub(crate) use loading::animation_asset;
pub(crate) use loading::{
    animation_asset_from_documents, parse_animation_document, read_animation_bytes,
};
pub(crate) use runtime::install_animation_asset;
pub(crate) use skins::animation_skin_filename;
#[cfg(test)]
pub(crate) use skins::parse_animation_skins;
