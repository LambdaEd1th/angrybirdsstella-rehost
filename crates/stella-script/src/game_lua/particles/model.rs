//! Packed particle fields shared by native update and draw members.

use crate::{BoundCompositePart, ResourceRuntime, SpriteCatalogRegion};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct Particle {
    pub(crate) sprite: String,
    pub(crate) sprites: Vec<String>,
    /// ParticleData+0x20. Purple resolves and retains the concrete
    /// AtlasSprite when this particle is created (and whenever a lifeTime
    /// animation changes frame); its draw member never looks the name up
    /// again.
    pub(crate) bound_region: Option<Arc<SpriteCatalogRegion>>,
    /// ParticleData+0x28. Atlas lookup has priority. When a later animation
    /// frame resolves to an atlas sprite Purple leaves an older composite
    /// pointer retained in this lower-priority slot, so keep the two native
    /// ownership fields separate instead of collapsing them into an enum.
    pub(crate) bound_composite: Option<Arc<Vec<BoundCompositePart>>>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) velocity_x: f32,
    pub(crate) velocity_y: f32,
    pub(crate) gravity_x: f32,
    pub(crate) gravity_y: f32,
    pub(crate) angle: f32,
    pub(crate) angular_velocity: f32,
    pub(crate) scale_begin: f32,
    pub(crate) scale_end: f32,
    pub(crate) current_scale: f32,
    pub(crate) elapsed: f32,
    pub(crate) lifetime: f32,
    pub(crate) animation_frame: usize,
    pub(crate) animate_over_lifetime: bool,
    pub(crate) mode: i32,
    // ParticleData+0x5C is consumed by ThemeParticleSystem as `(1-z)` during
    // acceleration, displacement, and scale interpolation.
    pub(crate) z: f32,
    // ParticleData+0x58 routes the derived-system virtual add into its
    // per-layer vector map at +0xB8.
    pub(crate) theme_layer_index: i32,
    pub(crate) ignore_time_multiplier: bool,
}

impl Particle {
    /// Mirror the resource binding tail of `sub_10008E524` and both frame-
    /// change branches in `sub_100091834`: overwrite +0x20 on every bind,
    /// but touch +0x28 only when the atlas lookup returned null.
    pub(crate) fn bind_sprite(&mut self, resources: &ResourceRuntime, data_root: &Path) {
        self.bound_region = resources
            .active_atlas_catalog_region(&self.sprite, data_root)
            .map(Arc::new);
        if self.bound_region.is_none() {
            self.bound_composite = resources.active_bound_composite(&self.sprite).map(Arc::new);
        }
    }

    /// Deferred wgpu commands need an explicit missing marker so a null
    /// native pointer pair cannot be rebound by a same-named resource loaded
    /// after the particle was submitted.
    pub(crate) fn draw_bindings(
        &self,
    ) -> (
        Option<Arc<SpriteCatalogRegion>>,
        Option<Arc<Vec<BoundCompositePart>>>,
    ) {
        let bound_region = self.bound_region.clone();
        if bound_region.is_some() {
            // Both native ownership slots may be populated after a
            // composite-to-atlas lifeTime frame transition, but both draw
            // members branch on +0x20 first and never visit +0x28.
            return (bound_region, None);
        }
        let mut bound_composite = self.bound_composite.clone();
        if bound_composite.is_none() {
            bound_composite = Some(Arc::new(Vec::new()));
        }
        (bound_region, bound_composite)
    }
}
