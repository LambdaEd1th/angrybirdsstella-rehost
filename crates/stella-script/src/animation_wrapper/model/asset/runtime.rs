//! Installation of a decoded asset into the recovered animation runtime.

use super::super::*;
use crate::{NativeSpriteMetrics, SpriteCatalogRegion};

pub(crate) fn install_animation_asset(
    runtime: &mut AnimationRuntime,
    tag: String,
    asset: AnimationAsset,
    sprite_geometry: BTreeMap<String, SpriteGeometry>,
    sprite_metrics: BTreeMap<String, NativeSpriteMetrics>,
    sprite_regions: BTreeMap<String, SpriteCatalogRegion>,
) {
    let selected_skin = if asset.definition.skins.contains_key("default") {
        Some("default".to_owned())
    } else {
        asset.definition.skins.keys().next().cloned()
    };
    // Replacing a component destroys its native per-component event vector.
    // Do not let events queued by the old scene instance leak into the new
    // asset that happens to reuse the same wrapper tag.
    runtime.pending_events.remove(&tag);
    runtime.pending_event_tags.retain(|pending| pending != &tag);
    runtime.actions.insert(tag.clone(), asset.actions);
    let loaded_playback = AnimationPlayback::loaded(&asset.definition.slots);
    runtime.definitions.insert(tag.clone(), asset.definition);
    runtime.sprite_geometry.insert(tag.clone(), sprite_geometry);
    runtime.sprite_metrics.insert(tag.clone(), sprite_metrics);
    runtime.sprite_regions.insert(tag.clone(), sprite_regions);
    runtime.transforms.entry(tag.clone()).or_default();
    runtime.matrices.entry(tag.clone()).or_default();
    runtime
        .descendant_reflections
        .entry(tag.clone())
        .or_default();
    if let Some(skin) = selected_skin {
        runtime.skins.insert(tag.clone(), skin);
    } else {
        runtime.skins.remove(&tag);
    }
    // sub_100010340 builds the scene and its components but never chooses an
    // action or creates the wrapper's current-control record at +0x30. Keep a
    // loaded scene state with no current control; start() creates the first
    // control later through sub_100012F18.
    runtime.playback.insert(tag, loaded_playback);
}
