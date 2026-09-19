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
    if !runtime.root_present {
        runtime.root_generation += 1;
        runtime.root_present = true;
    }
    runtime.next_scene_generation += 1;
    runtime
        .scene_generations
        .insert(tag.clone(), runtime.next_scene_generation);
    let selected_skin = if asset.definition.skins.contains_key("default") {
        Some("default".to_owned())
    } else {
        asset.definition.skins.keys().next().cloned()
    };
    runtime
        .skin_sets
        .insert(tag.clone(), asset.definition.skins.clone());
    // Replacing a component destroys its native per-component event vector.
    // Do not let events queued by the old scene instance leak into the new
    // asset that happens to reuse the same wrapper tag.
    runtime.pending_events.remove(&tag);
    runtime.pending_event_tags.retain(|pending| pending != &tag);
    runtime.actions.insert(tag.clone(), asset.actions);
    let mut loaded_playback = AnimationPlayback::loaded(&asset.definition.slots);
    // load's builder never erases the wrapper's +0x30 Control map. The old
    // scene no longer owns an active vector after removal, but the wrapper
    // retains its current Control until a later start/close replaces it.
    if let Some(control) = runtime
        .playback
        .get(&tag)
        .and_then(AnimationPlayback::current_control)
    {
        loaded_playback.current_action = control.action.clone();
        loaded_playback.detached_current = Some(control.clone());
        loaded_playback.wrapper_control_present = true;
    }
    runtime.definitions.insert(tag.clone(), asset.definition);
    runtime.sprite_geometry.insert(tag.clone(), sprite_geometry);
    runtime.sprite_metrics.insert(tag.clone(), sprite_metrics);
    runtime.sprite_regions.insert(tag.clone(), sprite_regions);
    // sub_100010340 constructs both replacement entities anew through
    // sub_10043AF5C; transforms and descendant reflection belong to those
    // identities, not to the reusable wrapper tag.
    runtime
        .transforms
        .insert(tag.clone(), AnimationTransform::default());
    runtime
        .matrices
        .insert(tag.clone(), AnimationAffine::default());
    runtime.descendant_reflections.insert(tag.clone(), false);
    runtime.shaders.remove(&tag);
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
