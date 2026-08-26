//! `ResourceManager` behavior recovered from Purple's native resource API.

mod audio_playback_registration;
mod audio_setup_registration;
mod draw_registration;
mod fonts;
mod geometry;
mod legacy_registration;
mod legacy_usage;
mod lifecycle_registration;
mod locale_font_registration;
mod localization;
mod query_registration;
mod registration;
mod runtime;

pub(crate) use audio_playback_registration::{native_play_audio, require_audio_output};
pub(crate) use draw_registration::native_resource_sprite_command;
pub(super) use lifecycle_registration::load_sprite_sheet_path;
pub(crate) use locale_font_registration::resolve_localized_string;
#[cfg(test)]
pub(crate) use registration::{REGISTERED_LEGACY_RESOURCE_METHODS, REGISTERED_RESOURCE_METHODS};
pub(crate) use registration::{RegistrationContext, install};

pub(super) use fonts::{
    FontMetric, SystemFontState, bitmap_font_metric, bitmap_font_string_width,
    create_system_font_state, load_bitmap_fonts, native_clip_text_lines,
    platform_system_font_names, system_font_color_from_lua, system_font_metric,
    system_font_string_width,
};
#[cfg(test)]
pub(crate) use geometry::load_sprite_geometry;
pub(super) use geometry::{
    NativeSpriteMetrics, NativeSpritePlacement, ParsedSpriteDraw, SpriteGeometry,
    SpriteHorizontalAnchor, SpriteVerticalAnchor, composite_part_lua_table,
    native_composite_metrics, native_composite_metrics_from_parts, parse_draw_sprite_args,
    sprite_draw_anchor_offset_from_geometry, update_composite_part_from_lua,
};
pub(super) use localization::{
    LocaleRuntime, load_localized_strings, localization_table_has_locale,
    localized_string_groups_from_table, resource_double_file_stem, resource_file_extension,
    resource_file_stem, resource_join_path, resource_normalized_path,
};
pub(super) use runtime::{
    AudioAssetState, AudioIoConfiguration, AudioRuntime, CompositeAudioState, ResourceRuntime,
};
