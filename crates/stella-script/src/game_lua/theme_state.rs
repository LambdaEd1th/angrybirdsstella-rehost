//! Native theme-layer and ThemeSpriteData runtime state.

use crate::SpriteGeometry;

/// Camera values cached by `ThemeManager::update` (`sub_10009AA4C`).  Purple
/// keeps these as float32 even though Lua numbers are doubles.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ThemeCameraReference {
    /// The native manager initializes the reference point lazily on its first
    /// update after `native_refreshThemeSystem`.
    pub(crate) valid: bool,
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// ThemeManager+0x70, selected from `castleCameraData.ipad` and falling
    /// back to `referenceCamera` when the iPad entry is absent.
    pub(crate) scale: f32,
    /// ThemeManager+0x6C. `sub_1000985DC` divides
    /// `originalCameras[2].sx` by the reference-camera `sx` and later uses
    /// that ratio when `relativeY` replaces native layer+0x40.
    pub(crate) original_scale_ratio: f32,
    /// ThemeManager+0x54/+0x58.  These are zero until the native camera-effect
    /// bridge publishes a shake displacement.
    pub(crate) effect_x: f32,
    pub(crate) effect_y: f32,
    /// ThemeManager+0xB0.  The iOS portrait target reports zero in the normal
    /// gameplay orientation, but retaining the slot keeps the recovered
    /// position helper structurally complete.
    pub(crate) orientation: f32,
}

impl Default for ThemeCameraReference {
    fn default() -> Self {
        Self {
            valid: false,
            x: 0.0,
            y: 0.0,
            // Synthetic binding tests do not run the level refresh chain.
            // Twenty preserves their legacy physics-scale fixture; shipped
            // levels always replace it with their authored camera scale.
            scale: 20.0,
            original_scale_ratio: 1.0,
            effect_x: 0.0,
            effect_y: 0.0,
            orientation: 0.0,
        }
    }
}

/// Scale interpolation at `0x10009C0DC..0x10009C114`.  Keeping the operation
/// sequence in float32 matters for both repeat counts and native culling.
pub(crate) fn native_theme_parallax_scale(
    current_scale: f32,
    end_scale: f32,
    reference_scale: f32,
    z_distance: f32,
) -> f32 {
    let end_over_reference = end_scale / reference_scale;
    let current_over_end = current_scale / end_scale;
    let one_minus_z = 1.0_f32 - z_distance;
    let current_term = current_over_end * (end_over_reference * one_minus_z);
    end_over_reference.mul_add(z_distance, current_term)
}

/// `0x10009B4C8..0x10009B4F8`: a finite `relativeY` replaces the layer's
/// normal Y offset on every ThemeManager draw pass. The AArch64 `FNMSUB`
/// computes `relative_y * height - height * 0.5` with one rounding.
pub(crate) fn native_theme_relative_y_offset(
    relative_y: f32,
    screen_height: f32,
    original_scale_ratio: f32,
) -> f32 {
    let half_height = screen_height * 0.5_f32;
    relative_y.mul_add(screen_height, -half_height) / original_scale_ratio
}

/// One authored `animationTimeline` entry. A scalar has zero variance; a
/// two-element table is sampled as `base + cmwc() * variance` in float32.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThemeAnimationTimelineEntry {
    pub(crate) base: f32,
    pub(crate) variance: f32,
    /// Native code consumes CMWC for every table entry even when the second
    /// value is absent or zero; scalar entries consume no sample.
    pub(crate) uses_random: bool,
}

/// Optional `spawnParameters.area` coordinates used by the layer-list
/// expander (`sub_10006B9A4`) and the animation-wrap refresh
/// (`sub_100099C24`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThemeSpawnArea {
    pub(crate) screen_x: f32,
    pub(crate) screen_y: f32,
    pub(crate) screen_width: f32,
    pub(crate) screen_height: f32,
    pub(crate) world_x: Option<f32>,
    pub(crate) world_y: Option<f32>,
    pub(crate) world_width: Option<f32>,
    pub(crate) world_height: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ThemeSpawnParameters {
    pub(crate) amount: i32,
    pub(crate) x_speed_variance: f32,
    pub(crate) y_speed_variance: f32,
    pub(crate) area: ThemeSpawnArea,
}

#[derive(Debug, Clone)]
pub(crate) struct ThemeLayer {
    pub(crate) sprite: String,
    pub(crate) geometry: SpriteGeometry,
    pub(crate) animation_frames: Vec<String>,
    pub(crate) animation_geometries: Vec<SpriteGeometry>,
    pub(crate) animation_delay: f64,
    /// Native layer+0xD0 owns a vector of per-frame float32 delays. The
    /// scalar `animationSpeed` at +0xFC remains the fallback when the current
    /// frame index is beyond this vector.
    pub(crate) animation_timeline: Vec<f32>,
    pub(crate) animation_timeline_definition: Vec<ThemeAnimationTimelineEntry>,
    pub(crate) animation_timer: f64,
    pub(crate) animation_frame: usize,
    /// One-based index of the source layer definition, stored at native
    /// layer+0x88. Spawn-expanded records intentionally share this index.
    pub(crate) definition_index: usize,
    /// Layer+0xE8 string consumed by ThemeManager::setTheme when constructing
    /// the per-layer ThemeParticleSystem Spawner.
    pub(crate) particles: Option<String>,
    /// Lua `spawnInterval`; missing values become the native -1.0 sentinel,
    /// which disables timed emission but keeps force-spawn available.
    pub(crate) spawn_interval: f32,
    /// Native layer+0xF8. `ThemeSystem::spawn*LayerParticles` scans the
    /// expanded layer vector for the first record with this authored ID.
    pub(crate) spawner_id: i32,
    /// Cached authored parameters used by the animation-wrap refresh. The
    /// shipped 1.1.6 definitions are immutable after `setTheme`.
    pub(crate) spawn_parameters: Option<ThemeSpawnParameters>,
    /// Native layer+0x34/+0x38 (`posX`/`posY`). ThemeSpriteData consumes this
    /// coordinate space separately from the drawable offsets below.
    pub(crate) position_x: f64,
    pub(crate) position_y: f64,
    pub(crate) offset_x: f64,
    pub(crate) offset_y: ThemeVerticalOffset,
    /// `sub_1000985DC` keeps the authored Lua `"top"`/`"bottom"` token in
    /// the theme table but overwrites native layer+0x40 with a camera-derived
    /// float.  Keeping both values mirrors that split ownership: the enum is
    /// the authored token and this slot is the refreshed native value.
    pub(crate) resolved_offset_y: Option<f64>,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    /// Native layer+0x18 (`parallaxSpeed`). Purple 1.1.6 parses and copies
    /// this float32 slot, but none of the GameLua or ThemeManager consumers
    /// reads it; live parallax motion uses `(1 - zDistance)` directly.
    #[allow(dead_code)]
    pub(crate) parallax_speed: f64,
    pub(crate) z_distance: f64,
    /// Native layer+0x44 (`scaleSpeed`). This is distinct from the nested
    /// ThemeSpriteData scale velocity and is dormant in Purple 1.1.6.
    #[allow(dead_code)]
    pub(crate) scale_speed: f64,
    /// Native layer+0x4C (`angleMult`). Retained for the exact record ABI;
    /// the shipped updater and renderer do not consume it.
    #[allow(dead_code)]
    pub(crate) angle_multiplier: f64,
    /// Native layer+0x50 (`xMult`). With the internal horizontal-anchor bit
    /// clear, `sub_10009CEB0` multiplies camera X displacement by
    /// `zDistance + xMult`.
    pub(crate) x_multiplier: f64,
    /// Native layer+0x54 (`yMult`). Unlike xMult, Purple 1.1.6 never reads
    /// this slot after construction; vertical camera displacement uses only
    /// zDistance (or the internal ANCHOR_V branch).
    #[allow(dead_code)]
    pub(crate) y_multiplier: f64,
    pub(crate) alpha: f64,
    pub(crate) min_alpha: f64,
    pub(crate) max_alpha: f64,
    pub(crate) repeat_x: bool,
    pub(crate) repeat_y: bool,
    pub(crate) repeat_left_only: bool,
    pub(crate) repeat_right_only: bool,
    /// Native layer+0x68. Besides repeat traversal, `sub_10009CEB0` consumes
    /// the vertical anchor bits while calculating camera-relative position.
    pub(crate) native_flags: u32,
    /// Native layer+0x120/+0x124. Purple 1.1.6 parses both fields using
    /// `FLT_MAX` as the missing sentinel. ThemeManager consumes only
    /// `relativeY`; retaining `relativeX` still preserves the record ABI.
    #[allow(dead_code)]
    pub(crate) relative_x: Option<f64>,
    pub(crate) relative_y: Option<f64>,
    /// Native layer+0x104..+0x110. Missing values use the same FLT_MAX
    /// sentinel as relativeX/Y; `sub_10009A894` consumes them in X, Y, W, H
    /// order before the relativeY overwrite.
    pub(crate) world_x: Option<f64>,
    pub(crate) world_y: Option<f64>,
    pub(crate) world_width: Option<f64>,
    pub(crate) world_height: Option<f64>,
    pub(crate) velocity_x: f64,
    pub(crate) velocity_y: f64,
    pub(crate) motion_y: f64,
}

/// Live Lua world limits read by `sub_10009AA4C` before its per-layer
/// `sub_10009A894` pass.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThemeWorldLimits {
    pub(crate) left: Option<f32>,
    pub(crate) right: Option<f32>,
    pub(crate) top: Option<f32>,
    pub(crate) bottom: Option<f32>,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeThemeSprite {
    pub(crate) sprite: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
    pub(crate) angular_velocity: f64,
    // ThemeSpriteData+0x54 is retained by the constructor contract. Purple
    // 1.1.6 has no draw traversal for this record vector, so the renderer
    // never consumes the flip bit.
    #[allow(dead_code)]
    pub(crate) horizontal_flip: bool,
    pub(crate) velocity_x: f64,
    pub(crate) velocity_y: f64,
    pub(crate) scale_speed: f64,
    pub(crate) original_x: f64,
    pub(crate) original_y: f64,
    // Stored at ThemeSpriteData+0x20 by sub_100055650. The recovered native
    // update at sub_1000607E8 does not read this field, but retaining it keeps
    // the constructor contract explicit for later renderer-side auditing.
    #[allow(dead_code)]
    pub(crate) animation_start_timer: f64,
    pub(crate) is_animation: bool,
    pub(crate) animation_frames: Vec<String>,
    pub(crate) animation_frame_time: f64,
    pub(crate) animation_timer: f64,
    pub(crate) animation_frame: usize,
    pub(crate) animation_looping: bool,
}

/// Purple owns one `std::vector<ThemeSpriteData>` per theme layer. The
/// flattened representation retains the combined layer index while preserving
/// vector insertion order and duplicate names.
#[derive(Debug, Default)]
pub(crate) struct NativeThemeSprites(Vec<((usize, String), NativeThemeSprite)>);

impl NativeThemeSprites {
    pub(crate) fn insert(&mut self, key: (usize, String), sprite: NativeThemeSprite) {
        self.0.push((key, sprite));
    }

    pub(crate) fn get(&self, key: &(usize, String)) -> Option<&NativeThemeSprite> {
        self.0
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, sprite)| sprite)
    }

    pub(crate) fn get_mut(&mut self, key: &(usize, String)) -> Option<&mut NativeThemeSprite> {
        self.0
            .iter_mut()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, sprite)| sprite)
    }

    pub(crate) fn remove(&mut self, key: &(usize, String)) -> Option<NativeThemeSprite> {
        let index = self.0.iter().position(|(candidate, _)| candidate == key)?;
        Some(self.0.remove(index).1)
    }

    pub(crate) fn remove_index(&mut self, index: usize) -> NativeThemeSprite {
        self.0.remove(index).1
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, ((usize, String), NativeThemeSprite)> {
        self.0.iter()
    }

    pub(crate) fn iter_mut(
        &mut self,
    ) -> std::slice::IterMut<'_, ((usize, String), NativeThemeSprite)> {
        self.0.iter_mut()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut NativeThemeSprite> {
        self.0.iter_mut().map(|(_, sprite)| sprite)
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &(usize, String)> {
        self.0.iter().map(|(key, _)| key)
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, key: &(usize, String)) -> bool {
        self.get(key).is_some()
    }
}

impl std::ops::Index<&(usize, String)> for NativeThemeSprites {
    type Output = NativeThemeSprite;

    fn index(&self, key: &(usize, String)) -> &Self::Output {
        self.get(key).expect("theme sprite key not found")
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ThemeVerticalOffset {
    Pixels(f64),
    Top,
    Bottom,
}
