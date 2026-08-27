//! Native `GameLua` subsystems recovered from Purple's registration object.
//!
//! Modules here follow the ownership boundaries visible in the executable,
//! while `lib.rs` remains the public compatibility facade.

mod arguments;
mod audio_registration;
mod bootstrap;
mod direct_sprite_registration;
mod draw_registration;
mod frame_update;
mod host;
mod host_audio;
mod host_frame;
mod host_input;
mod host_lifecycle;
mod host_output;
mod host_physics;
mod host_scene_sync;
mod host_startup;
mod input;
mod level_editor_registration;
mod level_failure_registration;
mod level_files;
mod level_load_registration;
mod level_save_registration;
mod level_save_schema;
mod level_table_clone;
mod loader_registration;
mod math_random;
mod native_lua_objects;
mod object_api;
mod object_body_registration;
mod object_decoration_registration;
mod object_feature_registration;
mod object_joint_registration;
mod object_lifecycle_registration;
mod object_material_registration;
mod object_motion_registration;
mod object_parameter_registration;
mod object_physics_registration;
mod object_physics_scale_registration;
mod object_pose_registration;
mod object_query_registration;
mod object_scale_member;
mod object_transform_registration;
mod object_visual_registration;
mod particle_registration;
mod particles;
mod persistence;
mod platform;
mod platform_services;
mod primitive_render_registration;
mod registration;
mod registration_inventory;
mod render_api;
mod render_bridge;
mod render_primitives;
mod runtime_state;
mod scene_render;
mod script_runtime;
mod simple_random;
mod text_files;
mod textured_render_registration;
mod theme_animation_registration;
mod theme_arguments;
mod theme_layer_parser;
mod theme_objects;
mod theme_render_registration;
mod theme_sprite_registration;
mod theme_state;
mod theme_system_registration;
mod theme_world_offsets;
mod time;
mod time_registration;
mod trajectory;
mod trajectory_bridge;
mod trajectory_registration;
mod ui_float_precision;
mod ui_text_registration;
mod world_environment_registration;
mod world_physics_camera_registration;
mod world_registration;
mod world_transform_registration;

pub(crate) use arguments::*;
pub(crate) use audio_registration::install as install_audio_bindings;
pub(crate) use bootstrap::install as install_bootstrap_globals;
pub(crate) use direct_sprite_registration::install as install_direct_sprite_bindings;
pub(crate) use draw_registration::install as install_draw_bindings;
pub use host::StellaLua;
#[cfg(test)]
pub(crate) use host_input::lock_native_pinch_for_test;
#[cfg(test)]
pub(crate) use input::NATIVE_FRAME_KEYS;
pub(crate) use input::{
    NativeKeyBuffers, install_input_queries, publish_native_key_state, trace_input_tables,
};
pub(super) use level_files::install as install_level_files;
pub(crate) use loader_registration::install as install_loader_bindings;
pub(crate) use math_random::NativeLibcRandom;
pub(crate) use math_random::install as install_math_random;
pub(crate) use native_lua_objects::{
    NativeLuaObject, native_lua_object, retain_constructor_lua_objects, retain_native_lua_object,
};
pub(super) use object_api::install as install_object_api;
pub(crate) use object_api::object_world;
pub(crate) use object_body_registration::install as install_object_body_bindings;
pub(crate) use object_feature_registration::install as install_object_feature_bindings;
pub(crate) use object_material_registration::install as install_object_material_bindings;
pub(crate) use object_motion_registration::install as install_object_motion_bindings;
pub(crate) use object_physics_registration::install as install_object_physics_bindings;
pub(crate) use object_query_registration::install as install_object_query_bindings;
pub(crate) use object_transform_registration::install as install_object_transform_bindings;
pub(crate) use object_visual_registration::install as install_object_visual_bindings;
pub(crate) use particle_registration::install as install_particle_bindings;
pub(crate) use particles::{
    NativeParticleRandom, NativeParticles, NativeThemeParticles, Particle, ParticleDefinition,
    spawn_particles,
};
pub(super) use persistence::{
    decode_persistent_lua, install_persistent_save, install_table_files, load_saved_lua_table,
    write_saved_lua_table,
};
pub(super) use platform::install as install_platform;
#[cfg(test)]
pub(crate) use platform::set_screenshot_sequence_for_test;
#[cfg(test)]
pub(crate) use platform::sha1_upper_hex;
pub(super) use platform::{
    InstalledAppsRuntime, UrlRequestRuntime, dispatch_installed_apps, dispatch_url_completions,
};
pub(crate) use platform_services::announce_cloud_service_registrations;
pub(crate) use platform_services::complete_iap_initialization;
pub(crate) use platform_services::install as install_platform_service_tables;
pub(crate) use platform_services::install_offline_game_server_facade;
pub(crate) use platform_services::{
    AssetsRuntime, ChannelRuntime, GameServerRuntime, GamerServicesRuntime, IapRuntime,
    SkynestStorageRuntime, dispatch_assets_completions, dispatch_channel_completions,
    dispatch_game_server_completions, dispatch_gamer_services_completions,
    dispatch_iap_completions, dispatch_skynest_storage_completions,
    load_shipped_game_server_facade,
};
pub(crate) use primitive_render_registration::install as install_primitive_render_bindings;
pub(crate) use registration::install_base_globals;
pub(crate) use registration_inventory::NATIVE_NOOP_FUNCTIONS;
#[cfg(test)]
pub(crate) use registration_inventory::{REGISTERED_GLOBAL_FUNCTIONS, REGISTERED_TABLE_FUNCTIONS};
pub(super) use render_api::{
    RegistrationContext as RenderRegistrationContext, install as install_render_api,
};
pub(crate) use render_primitives::*;
pub(crate) use runtime_state::*;
pub(crate) use scene_render::{NativeSceneRenderIndex, SceneDrawVisit, native_scene_sheet_id};
pub(crate) use script_runtime::*;
#[cfg(test)]
pub(super) use simple_random::NativeSeedRandom;
pub(super) use simple_random::install as install_simple_random;
pub(super) use text_files::{install_data_imports, install_string_loader};
pub(crate) use textured_render_registration::install as install_textured_render_bindings;
pub(crate) use theme_arguments::{
    native_theme_layer, theme_required_bool, theme_required_f32, theme_required_string,
    theme_table_bool, theme_table_f32, theme_table_string,
};
pub(crate) use theme_layer_parser::{named_theme_layer_offsets, parse_theme_layers};
pub(super) use theme_objects::install as install_theme_objects;
pub(crate) use theme_render_registration::install as install_theme_render_bindings;
pub(crate) use theme_state::{
    NativeThemeSprite, NativeThemeSprites, ThemeAnimationTimelineEntry, ThemeCameraReference,
    ThemeLayer, ThemeSpawnArea, ThemeSpawnParameters, ThemeVerticalOffset, ThemeWorldLimits,
    native_theme_parallax_scale, native_theme_relative_y_offset,
};
pub(crate) use theme_world_offsets::{
    ThemeWorldOffsetContext, live_theme_world_limits, refresh_theme_layer_world_offset,
};
pub(crate) use time::{
    add_duration_to_time_table, current_time_table, current_utc_time_table, time_table_seconds,
};
pub(crate) use time_registration::install as install_time_bindings;
pub(crate) use trajectory::*;
pub(crate) use trajectory_registration::install as install_trajectory_bindings;
pub(crate) use ui_text_registration::install as install_ui_text_bindings;
pub(crate) use world_registration::install as install_world_bindings;
