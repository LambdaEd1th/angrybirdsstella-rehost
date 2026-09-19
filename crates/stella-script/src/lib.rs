//! Lua 5.1 compatibility runtime for the original game scripts.

mod account_types;
mod animation_wrapper;
mod apprater_types;
mod audio_types;
mod device_info;
mod facebook_graph;
mod facebook_oauth;
mod game_lua;
pub mod mpg123_compat;
mod physics_world;
mod preferred_languages;
mod render_types;
mod resource_manager;
mod sdk_log_types;
mod social_types;
pub use facebook_graph::FacebookGraphSession;
pub use facebook_oauth::{
    FacebookLoginDialogAdapter, FacebookLoginDialogEvent, FacebookLoginDialogRequest,
    FacebookOAuthConfig, FacebookOAuthSession, FacebookSessionState, FacebookSystemAccountAdapter,
    FacebookSystemAccountCompletion, FacebookSystemAuthorization, FacebookTokenCache,
    FacebookTokenCacheError,
};
pub use social_types::{
    SocialFriendDetails, SocialLoginRequest, SocialNetwork, SocialPlatformDispatcher,
    SocialPlatformError, SocialPlatformFriends, SocialPlatformProfile, SocialPlatformProvider,
    SocialPlatformRequestOwner, SocialPlatformTask, SocialPlatformUser, SocialProfileRequest,
};

pub use account_types::{
    AccountFieldError, AccountGender, AccountUiAction, AccountUiSnapshot, AccountValidationField,
    AccountValidationResult, AccountView, RegistrationBirthday,
};
use animation_wrapper::*;
pub use apprater_types::{AppRatingButton, AppRatingChoice, AppRatingPrompt};
pub use audio_types::{
    AudioAssetSource, AudioOutputClock, AudioOutputState, AudioPlaybackState,
    AudioPlaybackTransitions,
};
pub(crate) use device_info::native_device_info_model;
pub use game_lua::StellaLua;
use game_lua::*;
use physics_world::*;
pub use render_types::*;
#[cfg(test)]
use resource_manager::load_sprite_geometry;
use resource_manager::{
    AudioAssetState, AudioIoConfiguration, AudioRuntime, CompositeAudioState, FontMetric,
    LocaleRuntime, NativeSpriteMetrics, NativeSpritePlacement, ParsedSpriteDraw, ResourceRuntime,
    SpriteGeometry, SpriteHorizontalAnchor, SpriteVerticalAnchor, bitmap_font_metric,
    bitmap_font_string_width, composite_part_lua_table, create_system_font_state,
    load_bitmap_fonts, load_localized_strings, load_sprite_sheet_path,
    localization_table_has_locale, localized_string_groups_from_table, native_clip_text_lines,
    native_composite_metrics, native_resource_sprite_command, native_utf8_skipping_invalid,
    parse_draw_sprite_args, platform_system_font_names, resolve_localized_string,
    resource_double_file_stem, resource_file_extension, resource_file_stem, resource_join_path,
    resource_normalized_path, sprite_draw_anchor_offset_from_geometry, system_font_metric,
    system_font_string_width, update_composite_part_from_lua,
};
pub use sdk_log_types::{SdkLogLevel, SdkLogSnapshot};

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
