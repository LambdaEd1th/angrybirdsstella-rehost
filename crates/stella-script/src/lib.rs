//! Lua 5.1 compatibility runtime for the original game scripts.

mod animation_wrapper;
mod audio_types;
mod game_lua;
pub mod mpg123_compat;
mod physics_world;
mod render_types;
mod resource_manager;

use animation_wrapper::*;
pub use audio_types::{AudioAssetSource, AudioOutputClock, AudioOutputState, AudioPlaybackState};
pub use game_lua::StellaLua;
use game_lua::*;
use physics_world::*;
pub use render_types::*;
#[cfg(test)]
use resource_manager::load_sprite_geometry;
use resource_manager::{
    AudioAssetState, AudioIoConfiguration, AudioRuntime, CompositeAudioState, FontMetric,
    LocaleRuntime, NativeSpriteMetrics, NativeSpritePlacement, ResourceRuntime, SpriteGeometry,
    bitmap_font_metric, bitmap_font_string_width, composite_part_lua_table,
    create_system_font_state, load_bitmap_fonts, load_localized_strings, load_sprite_sheet_path,
    localization_table_has_locale, localized_string_groups_from_table, native_clip_text_lines,
    native_composite_metrics, parse_draw_sprite_args, platform_system_font_names,
    resolve_localized_string, resource_double_file_stem, resource_file_extension,
    resource_file_stem, resource_join_path, resource_normalized_path,
    sprite_draw_anchor_offset_from_geometry, system_font_metric, system_font_string_width,
    update_composite_part_from_lua,
};

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{
    Error as LuaError, HookTriggers, Lua, LuaSerdeExt, MultiValue, Result as LuaResult, Value,
    VmState,
};
use stella_assets::ka3d::CompositePart;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("script path is unsafe: {0}")]
    UnsafePath(String),
    #[error("script was not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("asset error: {0}")]
    Asset(#[from] stella_assets::AssetError),
    #[error("Lua error: {0}")]
    Lua(#[from] LuaError),
}

#[cfg(test)]
mod tests;
