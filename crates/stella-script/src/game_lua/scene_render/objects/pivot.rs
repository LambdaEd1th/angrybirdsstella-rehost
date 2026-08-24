//! Native atlas and CompoSprite pivots installed for scene-object callbacks.

use super::SceneDrawObject;
use crate::*;

impl SceneDrawObject {
    /// Recover the live GL-context pivot installed by `sub_10004BAB4` before
    /// the object's draw/callback branch.
    pub(crate) fn callback_pivot(&self) -> (f32, f32) {
        if let Some(parts) = self
            .composite_sprite
            .as_deref()
            .filter(|parts| !parts.is_empty())
        {
            // The composite branch at 0x10004C020 reads CompoSprite+0x68/
            // +0x6C directly and does not add RenderObjectData+0xB4/+0xB8.
            return native_composite_metrics(parts).map_or((0.0, 0.0), |metrics| {
                (metrics.pivot_x as f32, metrics.pivot_y as f32)
            });
        }
        self.sprite_region.as_ref().map_or((0.0, 0.0), |region| {
            // The atlas branch converts its signed 16-bit pivot and adds the
            // two float32 object offsets with one FADD per axis.
            (
                f32::from(region.sprite.pivot_x) + self.pivot_offset_x as f32,
                f32::from(region.sprite.pivot_y) + self.pivot_offset_y as f32,
            )
        })
    }
}
