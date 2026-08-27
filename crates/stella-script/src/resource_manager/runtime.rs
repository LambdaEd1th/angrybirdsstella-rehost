//! Mutable ResourceManager and audio-handle state behind the Lua adapters.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use stella_assets::ka3d::{BitmapFont, CompositeSpriteSet, LocalizationTable, SpriteSheet};

use super::{NativeSpriteMetrics, SystemFontState};
use crate::{
    AudioAssetSource, CompositeSpriteOwner, SpriteCatalogRegion, SpriteShader, TextFontBinding,
    resolve_data_file,
};

mod sprite_catalog;
mod sprite_lifecycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioIoConfiguration {
    pub(crate) channels: i32,
    pub(crate) bits_per_sample: i32,
    pub(crate) samples_per_second: i32,
    /// AudioOutputImpl rounds a 25 ms block to a sample-frame boundary and
    /// then to the next power of two before allocating its stream buffer.
    pub(crate) buffer_bytes: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct AudioClipState {
    pub(crate) name: String,
    pub(crate) asset: Option<AudioAssetState>,
    pub(crate) volume: f32,
    pub(crate) looping: bool,
    pub(crate) channel: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AudioAssetState {
    pub(crate) source: AudioAssetSource,
    pub(crate) duration: Option<Duration>,
    pub(crate) sample_frames: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompositeAudioState {
    pub(crate) parts: Vec<String>,
}

pub(crate) struct AudioRuntime {
    pub(crate) next_handle: u32,
    pub(crate) clips: BTreeMap<i64, AudioClipState>,
    pub(crate) track_volumes: [f32; 8],
    pub(crate) channel_limits: [i32; 8],
    pub(crate) composite_clips: BTreeMap<String, CompositeAudioState>,
    pub(crate) assets: BTreeMap<String, AudioAssetState>,
}

impl AudioRuntime {
    pub(crate) fn native_handle(handle: i64) -> i64 {
        i64::from(handle as u32)
    }

    pub(crate) fn play(&mut self, name: String, volume: f32, looping: bool, channel: i32) -> i64 {
        let Some(limit) = usize::try_from(channel)
            .ok()
            .and_then(|index| self.channel_limits.get(index))
        else {
            return -1;
        };
        let active_count = self
            .clips
            .values()
            .filter(|clip| clip.channel == channel)
            .count() as u32;
        if active_count >= *limit as u32 {
            return -1;
        }

        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        let lua_handle = i64::from(handle);
        let asset = self.assets.get(&name).cloned();
        self.clips.insert(
            lua_handle,
            AudioClipState {
                name,
                asset,
                volume,
                looping,
                channel,
            },
        );
        lua_handle
    }

    /// Reset the state owned by one native `AudioMixer` instance while
    /// retaining the `LuaResources` name-to-clip maps. `createAudioOutput`
    /// destroys the previous output (and therefore this mixer) before it
    /// constructs the replacement.
    pub(crate) fn reset_output_manager(&mut self) {
        self.next_handle = 0;
        self.clips.clear();
        self.track_volumes = [1.0; 8];
        self.channel_limits = [-1; 8];
    }

    /// Replace one named AudioClip pointer. Purple first marks every manager
    /// instance retaining the previous top-level pointer as finished, then
    /// overwrites the map node. Composite clips keep their separately retained
    /// child pointers and are therefore unaffected.
    pub(crate) fn replace_asset(&mut self, name: String, asset: Option<AudioAssetState>) {
        if self.assets.contains_key(&name) {
            self.clips.retain(|_, clip| clip.name != name);
        }
        if let Some(asset) = asset {
            self.assets.insert(name, asset);
        } else {
            self.assets.remove(&name);
        }
    }
}

impl Default for AudioRuntime {
    fn default() -> Self {
        Self {
            next_handle: 0,
            clips: BTreeMap::new(),
            track_volumes: [1.0; 8],
            channel_limits: [-1; 8],
            composite_clips: BTreeMap::new(),
            assets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpriteResourceKind {
    Atlas,
    Composite,
}

#[derive(Debug, Clone)]
pub(crate) struct SpriteResourceEntry {
    pub(crate) kind: SpriteResourceKind,
    pub(crate) owner: String,
    /// Index of the retained native Sprite object inside its owning resource.
    /// Purple's active-name stack stores the concrete Sprite pointer; keeping
    /// this index avoids rescanning the immutable SPRT/COMP vector by name.
    pub(crate) index: usize,
    /// Concrete Sprite fields read by getSpriteBounds/getSpritePivot.
    pub(crate) metrics: NativeSpriteMetrics,
    /// Concrete AtlasSprite owner stored at native resource-stack entry
    /// `+0x10`. Production SpriteSheet construction fills this once after the
    /// sheet's texture bindings have been resolved; active draw lookup then
    /// clones this pointer instead of searching the owning sheet again.
    pub(crate) atlas_region: Option<Arc<SpriteCatalogRegion>>,
    /// Concrete CompoSprite owner stored in the same native `+0x10` slot for
    /// type-two entries. Its Entry records remain mutable after scene objects
    /// and particles retain this owner.
    pub(crate) composite_sprite: Option<Arc<CompositeSpriteOwner>>,
}

#[derive(Debug)]
pub(crate) struct ResourceRuntime {
    pub(crate) path: String,
    pub(crate) sprite_sheets: BTreeSet<String>,
    /// Native SpriteSheet map value identity represented by its resolved file.
    pub(crate) sprite_sheet_paths: BTreeMap<String, String>,
    /// Concrete descriptor used to resolve the sheet-owned texture sources.
    pub(crate) sprite_sheet_descriptor_paths: BTreeMap<String, PathBuf>,
    pub(crate) sprite_sheet_values: BTreeMap<String, SpriteSheet>,
    /// Host bindings constructed with the native SpriteSheet resource.  Draw
    /// submission is intentionally lookup-only: resolving and canonicalizing
    /// a texture path for every sprite would turn Poppy's drill burst into
    /// hundreds of filesystem calls in one display-link frame.
    pub(crate) sprite_sheet_texture_sources: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) sprite_sheet_catalog_regions:
        BTreeMap<String, BTreeMap<String, Arc<SpriteCatalogRegion>>>,
    /// Address-order stand-in for native `SpriteSheet*` allocations. Draw
    /// buckets retain these ids independently of the active name map.
    pub(crate) sprite_sheet_identities: BTreeMap<String, u64>,
    pub(crate) next_sprite_sheet_identity: u64,
    /// Keys whose retained SpriteSheet has had its `+0x20` resource pointer
    /// cleared by `releaseSpriteSheet(path, true)`.
    pub(crate) released_sprite_sheet_resources: BTreeSet<String>,
    pub(crate) composite_sets: BTreeSet<String>,
    /// Native CompoSpriteSet map value identity represented by its resolved file.
    pub(crate) composite_set_paths: BTreeMap<String, String>,
    pub(crate) composite_set_values: BTreeMap<String, CompositeSpriteSet>,
    /// AtlasSprite pointers retained for every native CompoSprite Entry. Both
    /// vector levels stay index-aligned with CompositeSpriteSet::sprites/parts.
    pub(crate) composite_set_regions: BTreeMap<String, Vec<Vec<SpriteCatalogRegion>>>,
    /// Native `Resources + 0x580` maps each name to a priority vector. Lookup
    /// observes only the final entry and then applies any requested type gate.
    pub(crate) sprite_entries: BTreeMap<String, Vec<SpriteResourceEntry>>,
    /// Compatibility aliases for assets that the retired cloud asset service
    /// supplied at runtime.  An alias is materialized only in sheets that own
    /// its fallback target, so a later downloaded sheet with the real name
    /// still wins through the native last-entry-wins resource stack.
    pub(crate) sprite_aliases: BTreeMap<String, String>,
    pub(crate) bitmap_fonts: BTreeSet<String>,
    /// Bitmap IFont map values represented by their resolved source files.
    pub(crate) bitmap_font_paths: BTreeMap<String, String>,
    /// Concrete constructor input retained independently of later path/file
    /// changes, matching the native BitmapFont object's source identity.
    pub(crate) bitmap_font_descriptor_paths: BTreeMap<String, PathBuf>,
    /// Host equivalent of BitmapFont+0x50's retained Texture/AtlasSheet owner.
    /// Production text submission is lookup-only and never re-enters the
    /// filesystem to rediscover this constructor-resolved source.
    pub(crate) bitmap_font_texture_sources: BTreeMap<String, String>,
    /// Parsed bitmap object committed by `createBitmapFont`. Keeping the
    /// value here freezes the successful constructor input just like the
    /// native shared IFont object instead of reopening its file on queries.
    pub(crate) bitmap_font_values: BTreeMap<String, Arc<BitmapFont>>,
    pub(crate) system_fonts: BTreeMap<String, SystemFontState>,
    /// Generation of SystemFont's process-global LabelPool. Purple advances
    /// this lifetime boundary whenever its last SystemFont::Impl is destroyed.
    pub(crate) system_font_label_pool_epoch: u64,
    pub(crate) current_font: Option<String>,
    pub(crate) text_group_sets: BTreeSet<String>,
    /// Native TextGroupSet map value identity represented by its resolved file.
    pub(crate) text_group_set_paths: BTreeMap<String, String>,
    /// `None` is a constructed TextGroupSet whose post-commit parse threw.
    /// Purple replaces the map value before calling `sub_1004729A4`, unlike
    /// its other three resource constructors.
    pub(crate) text_group_set_tables: BTreeMap<String, Option<LocalizationTable>>,
    pub(crate) audio_clips: BTreeSet<String>,
    /// Per-sheet upload deltas accumulated by ResourceManager at native
    /// offset `+0x30` and published as `g_usedTextureMemory`.
    pub(crate) legacy_texture_usage: BTreeMap<String, u32>,
    /// Texture paths retained by each native sheet, used to reproduce the
    /// graphics cache's no-second-upload behavior for shared PVRs.
    pub(crate) legacy_sheet_textures: BTreeMap<String, Vec<String>>,
    pub(crate) legacy_texture_ref_counts: BTreeMap<String, u32>,
    /// Per-clip decoded byte counts at ResourceManager offset `+0x60`.
    pub(crate) legacy_audio_usage: BTreeMap<String, u32>,
    /// ResourceManager's name-to-play-count tree at native offset `+0x90`.
    /// Its `native_playAudio` member increments this even for a failed play.
    pub(crate) legacy_audio_play_counts: BTreeMap<String, u32>,
    /// GameLua's process-wide shader-name cache. `sub_10006CB08` mutates the
    /// cached shader before callers clone it into animation scenes or draw
    /// submissions, so omitted parameters inherit their previous values.
    pub(crate) shader_cache: BTreeMap<String, SpriteShader>,
    pub(crate) audio_output_created: bool,
    pub(crate) audio_output_started: bool,
    /// Host-side identity for the current native AudioOutputImpl allocation.
    /// Handles restart at zero whenever this generation advances.
    pub(crate) audio_output_generation: u64,
    pub(crate) audio_input_created: bool,
    pub(crate) audio_input_started: bool,
    pub(crate) audio_output_configuration: Option<AudioIoConfiguration>,
    pub(crate) audio_input_configuration: Option<AudioIoConfiguration>,
    pub(crate) master_volume: f32,
    pub(crate) clip_rect: [i32; 4],
    /// Deferred-host mirror revision for the native `Resources + 0x588` map.
    pub(crate) sprite_catalog_revision: u64,
}

impl Default for ResourceRuntime {
    fn default() -> Self {
        Self::new(1024, 768)
    }
}

impl ResourceRuntime {
    pub(crate) fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            path: String::new(),
            sprite_sheets: BTreeSet::new(),
            sprite_sheet_paths: BTreeMap::new(),
            sprite_sheet_descriptor_paths: BTreeMap::new(),
            sprite_sheet_values: BTreeMap::new(),
            sprite_sheet_texture_sources: BTreeMap::new(),
            sprite_sheet_catalog_regions: BTreeMap::new(),
            sprite_sheet_identities: BTreeMap::new(),
            next_sprite_sheet_identity: 1,
            released_sprite_sheet_resources: BTreeSet::new(),
            composite_sets: BTreeSet::new(),
            composite_set_paths: BTreeMap::new(),
            composite_set_values: BTreeMap::new(),
            composite_set_regions: BTreeMap::new(),
            sprite_entries: BTreeMap::new(),
            sprite_aliases: BTreeMap::new(),
            bitmap_fonts: BTreeSet::new(),
            bitmap_font_paths: BTreeMap::new(),
            bitmap_font_descriptor_paths: BTreeMap::new(),
            bitmap_font_texture_sources: BTreeMap::new(),
            bitmap_font_values: BTreeMap::new(),
            system_fonts: BTreeMap::new(),
            system_font_label_pool_epoch: 0,
            current_font: None,
            text_group_sets: BTreeSet::new(),
            text_group_set_paths: BTreeMap::new(),
            text_group_set_tables: BTreeMap::new(),
            audio_clips: BTreeSet::new(),
            legacy_texture_usage: BTreeMap::new(),
            legacy_sheet_textures: BTreeMap::new(),
            legacy_texture_ref_counts: BTreeMap::new(),
            legacy_audio_usage: BTreeMap::new(),
            legacy_audio_play_counts: BTreeMap::new(),
            shader_cache: BTreeMap::new(),
            audio_output_created: false,
            audio_output_started: false,
            audio_output_generation: 0,
            audio_input_created: false,
            audio_input_started: false,
            audio_output_configuration: None,
            audio_input_configuration: None,
            // No AudioOutputImpl exists yet. Construction writes the native
            // -1.0 "query OpenAL max gain on first start" sentinel.
            master_volume: -1.0,
            clip_rect: [0, 0, screen_width as i32, screen_height as i32],
            sprite_catalog_revision: 1,
        }
    }

    pub(crate) fn current_text_font_binding(
        &self,
        data_root: &std::path::Path,
    ) -> Option<(String, TextFontBinding)> {
        let name = self.current_font.clone()?;
        if let Some(font) = self.system_fonts.get(&name) {
            return Some((name, TextFontBinding::System(font.render_binding())));
        }
        let font = Arc::clone(self.bitmap_font_values.get(&name)?);
        let texture_source = self
            .bitmap_font_texture_sources
            .get(&name)
            .cloned()
            .unwrap_or_else(|| {
                // Direct ResourceRuntime fixtures can install a parsed font
                // without invoking createBitmapFont. Keep that diagnostic
                // path functional; production constructors always cache.
                let descriptor = self
                    .bitmap_font_descriptor_paths
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        self.bitmap_font_paths
                            .get(&name)
                            .and_then(|source| resolve_data_file(data_root, source).ok())
                    });
                sprite_catalog::resolve_texture_source(
                    data_root,
                    descriptor.as_ref(),
                    &font.texture,
                )
            });
        Some((
            name,
            TextFontBinding::Bitmap {
                font,
                texture_source,
            },
        ))
    }

    pub(crate) fn remove_system_font(&mut self, name: &str) -> Option<SystemFontState> {
        let removed = self.system_fonts.remove(name);
        if removed.is_some() && self.system_fonts.is_empty() {
            // Impl::~Impl at 0x100477BC4 erases the complete static LabelPool
            // exactly on the live-instance transition from one to zero.
            self.system_font_label_pool_epoch = self.system_font_label_pool_epoch.wrapping_add(1);
        }
        removed
    }
}
