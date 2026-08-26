//! Native atlas and CompoSprite pivots installed for scene-object callbacks.

use crate::*;

pub(super) fn native_scene_callback_pivot(
    composite_sprite: Option<&[BoundCompositePart]>,
    sprite_region: Option<&SpriteCatalogRegion>,
    pivot_offset_x: f64,
    pivot_offset_y: f64,
) -> (f32, f32) {
    if let Some(parts) = composite_sprite.filter(|parts| !parts.is_empty()) {
        // The composite branch at 0x10004C020 reads CompoSprite+0x68/
        // +0x6C directly and does not add RenderObjectData+0xB4/+0xB8.
        return native_composite_metrics(parts).map_or((0.0, 0.0), |metrics| {
            (metrics.pivot_x as f32, metrics.pivot_y as f32)
        });
    }
    sprite_region.map_or((0.0, 0.0), |region| {
        // The atlas branch converts its signed 16-bit pivot and adds the
        // two float32 object offsets with one FADD per axis.
        (
            f32::from(region.sprite.pivot_x) + pivot_offset_x as f32,
            f32::from(region.sprite.pivot_y) + pivot_offset_y as f32,
        )
    })
}
