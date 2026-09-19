//! Skin-slot lookup, attachment fallback and SpriteComponent canonicalization.

use std::collections::BTreeMap;

use super::super::model::*;
use super::hierarchy::animation_target_sample;

pub(crate) fn animation_skin_slot<'a>(
    skin: &'a AnimationSkin,
    slot: &str,
) -> Option<&'a BTreeMap<String, AnimationSkinTransform>> {
    skin.get(slot)
        .or_else(|| slot.strip_prefix("SLOT_").and_then(|name| skin.get(name)))
}

pub(crate) fn animation_slot_attachment(
    definition: &AnimationDefinition,
    playback: &AnimationPlayback,
    selected_skin_name: Option<&str>,
    slot: &str,
) -> Option<(String, Option<AnimationSkinTransform>)> {
    let sprite = animation_target_sample(definition, playback, slot, |target| {
        !target.sprite.is_empty()
    })
    .and_then(|(target, time)| sample_discrete(&target.sprite, time))
    .or_else(|| {
        playback
            .latched_targets
            .get(slot)
            .map(|target| target.sprite.clone())
    })?;
    if sprite.is_empty() {
        return None;
    }
    animation_skin_alias_attachment(&definition.skins, selected_skin_name, slot, &sprite)
}

pub(crate) fn animation_skin_alias_attachment(
    skins: &BTreeMap<String, AnimationSkin>,
    selected_skin_name: Option<&str>,
    slot: &str,
    sprite: &str,
) -> Option<(String, Option<AnimationSkinTransform>)> {
    if sprite.is_empty() {
        return None;
    }
    // The animation track stores the attachment alias, and that alias can be
    // namespaced (the chapter-two finale borders use
    // `borders_chapter_2_end/...`).  Skin lookup is performed with that exact
    // alias; only the resolved SpriteComponent name is canonicalized to its
    // basename.  Stripping the namespace before the lookup makes the native
    // component treat the attachment as absent and collapses ComicCutscene's
    // four-border clip rectangle to zero.
    let sprite_basename = sprite.rsplit('/').next().unwrap_or(sprite);
    let selected_skin = selected_skin_name.and_then(|name| skins.get(name));
    let default_skin = skins.get("default");
    let skin_transform = selected_skin
        .and_then(|skin| animation_skin_slot(skin, slot))
        .and_then(|variants| {
            variants
                .get(sprite)
                .or_else(|| variants.get(sprite_basename))
        })
        .or_else(|| {
            default_skin
                .and_then(|skin| animation_skin_slot(skin, slot))
                .and_then(|variants| {
                    variants
                        .get(sprite)
                        .or_else(|| variants.get(sprite_basename))
                })
        });
    if let Some(skin_transform) = skin_transform {
        // SpriteComponentCustom forwards the attachment string unchanged to
        // SpriteManager's case-sensitive std::map lookup (sub_100468574 ->
        // sub_100453BBC -> sub_10046B0B8).  This distinction is observable in
        // Poppy's shipped animation: the mixed-case `Poppy_idle` editor guide
        // intentionally does not resolve to the `POPPY_IDLE` gameplay sprite.
        return Some((skin_transform.sprite.clone(), Some(skin_transform.clone())));
    }
    let skin_managed_slot = skins
        .values()
        .any(|skin| animation_skin_slot(skin, slot).is_some());
    (!skin_managed_slot).then_some((sprite_basename.to_owned(), None))
}
