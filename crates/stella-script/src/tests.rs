//! Regression coverage grouped by recovered Purple native subsystems.
//!
//! Hopper reports `sub_10005E898` as one 6,476-byte/237-block GameLua
//! orchestration procedure. Its Box2D callees, registration families, render
//! helpers, themes, and AnimationWrapper live in distinct native clusters, so
//! keep their regression evidence in matching Rust modules.

use super::*;
use mlua::Function;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_SPRITE_SHEET_ID: AtomicU64 = AtomicU64::new(0);

/// Most subsystem tests enter at an already running GameLua frame rather
/// than replaying the shipped startup scripts. Release the constructor's
/// native unnamed physics lock explicitly for those fixtures.
fn unlocked_test_runtime() -> StellaLua {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source("g_outOfBoundariesObjects = {}; setPhysicsEnabled(true)")
        .unwrap();
    runtime
}

fn test_pcm_wav(data_len: u32) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0");
    bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x3e, 0, 0, 0, 0x7d, 0, 0, 2, 0, 16, 0]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(bytes.len() + data_len as usize, 0);
    bytes
}

fn test_ka3d(resource_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(resource_type);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn test_ka3d_string(value: &str) -> Vec<u8> {
    let mut bytes = (value.len() as u16).to_be_bytes().to_vec();
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn test_ka3d_chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn test_sprite_sheet() -> Vec<u8> {
    // Version one, an empty texture path, and no named region still construct
    // a native SPRT sheet, which is ideal for lifecycle-only tests.
    test_ka3d(b"SPRT", &[0, 1, 0, 0, 0, 0])
}

fn test_named_sprite_sheet(name: &str, width: u16, height: u16) -> Vec<u8> {
    test_sprite_sheet_with_names(&[(name, width, height)])
}

fn test_textured_sprite_sheet(name: &str, texture: &str, width: u16, height: u16) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string(texture));
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend(test_ka3d_string(name));
    for value in [0, 0, width, height, width / 2, height / 2] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    test_ka3d(b"SPRT", &payload)
}

fn test_sprite_sheet_with_names(entries: &[(&str, u16, u16)]) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend_from_slice(&0u16.to_be_bytes());
    payload.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for &(name, width, height) in entries {
        payload.extend(test_ka3d_string(name));
        for value in [0, 0, width, height, width / 2, height / 2] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
    }
    test_ka3d(b"SPRT", &payload)
}

fn test_textured_sprite_sheet_with_names(texture: &str, entries: &[(&str, u16, u16)]) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string(texture));
    payload.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for &(name, width, height) in entries {
        payload.extend(test_ka3d_string(name));
        for value in [0, 0, width, height, width / 2, height / 2] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
    }
    test_ka3d(b"SPRT", &payload)
}

fn register_test_sprite_sheet(runtime: &StellaLua, names: &[&str]) -> String {
    let entries = names
        .iter()
        .map(|name| (*name, 1_u16, 1_u16))
        .collect::<Vec<_>>();
    register_test_sprite_sheet_with_sizes(runtime, &entries)
}

fn register_test_sprite_sheet_with_sizes(
    runtime: &StellaLua,
    entries: &[(&str, u16, u16)],
) -> String {
    let unique = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("stella-test-sheet-{}-{unique}.dat", std::process::id());
    let path = runtime.data_root().join(&file_name);
    fs::write(
        &path,
        test_textured_sprite_sheet_with_names("stella-test-texture.pvr", entries),
    )
    .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let resources = environment.get::<mlua::Table>("res").unwrap();
    resources
        .get::<Function>("createSpriteSheet")
        .unwrap()
        .call::<()>((file_name.as_str(), true))
        .unwrap();
    fs::remove_file(path).unwrap();
    file_name
}

fn bind_test_animation_sprites(runtime: &mut AnimationRuntime, tag: &str, sprites: &[&str]) {
    runtime.sprite_regions.insert(
        tag.to_owned(),
        sprites
            .iter()
            .map(|name| {
                (
                    (*name).to_owned(),
                    SpriteCatalogRegion {
                        native_sheet_id: 1,
                        texture_source: "test-animation.pvr".to_owned(),
                        sprite: stella_assets::ka3d::SpriteRegion {
                            name: (*name).to_owned(),
                            x: 0,
                            y: 0,
                            width: 0,
                            height: 0,
                            pivot_x: 0,
                            pivot_y: 0,
                            atlas_rotation: 0,
                        },
                    },
                )
            })
            .collect(),
    );
}

fn test_composite_set(name: Option<&str>) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend_from_slice(&u16::from(name.is_some()).to_be_bytes());
    if let Some(name) = name {
        payload.extend(test_ka3d_string(name));
        payload.extend_from_slice(&0u16.to_be_bytes());
    }
    test_ka3d(b"COMP", &payload)
}

fn test_composite_set_with_part(composite: &str, sprite: &str) -> Vec<u8> {
    let mut payload = 2u16.to_be_bytes().to_vec();
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend(test_ka3d_string(composite));
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend(test_ka3d_string(sprite));
    payload.extend_from_slice(&0i16.to_be_bytes());
    payload.extend_from_slice(&0i16.to_be_bytes());
    payload.extend_from_slice(&0u16.to_be_bytes());
    test_ka3d(b"COMP", &payload)
}

fn test_bitmap_font() -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string(""));
    payload.extend_from_slice(&0i16.to_be_bytes());
    payload.extend_from_slice(&0i16.to_be_bytes());
    payload.extend_from_slice(&0u16.to_be_bytes());
    test_ka3d(b"FONT", &payload)
}

fn test_bitmap_font_with_glyph(texture: &str, width: u16) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string(texture));
    payload.extend_from_slice(&1i16.to_be_bytes());
    payload.extend_from_slice(&2i16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    for value in [u16::from(b'A'), 0, 0, width, 7, 5] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    test_ka3d(b"FONT", &payload)
}

fn test_localization_table(locale: &str, id: &str, translation: &str) -> Vec<u8> {
    let mut locales = 1u16.to_be_bytes().to_vec();
    locales.extend(test_ka3d_string(locale));
    let mut ids = 1u16.to_be_bytes().to_vec();
    ids.extend(test_ka3d_string(id));
    let translations = test_ka3d_string(translation);
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_chunk(b"LDAT", &locales));
    payload.extend(test_ka3d_chunk(b"LIDS", &ids));
    payload.extend(test_ka3d_chunk(b"TXGP", &translations));
    test_ka3d(b"TEXT", &payload)
}

struct ShippedDataSandbox {
    root: std::path::PathBuf,
    data_root: std::path::PathBuf,
}

impl ShippedDataSandbox {
    fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("stella-{label}-{unique}"));
        let data_root = root.join("data");
        fs::create_dir_all(root.join("appdata")).unwrap();
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../runtime/data")
            .canonicalize()
            .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(shipped, &data_root).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(shipped, &data_root).unwrap();
        Self { root, data_root }
    }
}

impl Drop for ShippedDataSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

mod animation;
mod audio_registration;
mod bird_run;
mod body_bindings;
mod broad_phase;
mod collision_callbacks;
mod construction;
mod contact_solver;
mod continuous_world;
mod data_loaders;
mod definitions;
mod dirt;
mod discrete_world;
mod global_render_state;
mod gravity_visuals;
mod joint_construction;
mod math_random;
mod narrow_phase;
mod object_motion;
mod object_render_state;
mod particles;
mod physics_queries;
mod platform_services;
mod position_constraints;
mod prismatic_joints;
mod pulley;
mod render_bindings;
mod render_submission;
mod resources;
mod revolute_joints;
mod rope_joints;
mod scene_render;
mod sensors;
mod shipped_levels;
mod themes;
mod tracks;
mod trajectory;
mod weld_joints;
