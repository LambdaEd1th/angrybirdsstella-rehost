//! Scene/theme rendering facade split at native member boundaries.

mod index;
mod objects;
mod theme;

pub(crate) use index::{NativeSceneRenderIndex, native_scene_sheet_id};
