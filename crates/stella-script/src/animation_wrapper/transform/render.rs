//! Native z-ordered animation slot expansion into affine render commands.

use crate::{NativeSpriteMetrics, RenderCommand, RenderState, SpriteCatalogRegion, SpriteGeometry};

use super::{
    super::model::*,
    affine::animation_node_world_affine,
    hierarchy::{animation_node_transform, animation_target_sample},
    skin::animation_slot_attachment,
};

pub(crate) fn animation_render_commands(
    runtime: &AnimationRuntime,
    tag: &str,
) -> Vec<RenderCommand> {
    let Some(playback) = runtime.playback.get(tag) else {
        return Vec::new();
    };
    let Some(definition) = runtime.definitions.get(tag) else {
        return Vec::new();
    };
    let root = runtime.transforms.get(tag).copied().unwrap_or_default();
    let root_affine = runtime
        .matrices
        .get(tag)
        .copied()
        .unwrap_or_else(|| AnimationAffine::from_transform(root));
    let sprite_geometry = runtime.sprite_geometry.get(tag);
    let sprite_metrics = runtime.sprite_metrics.get(tag);
    let sprite_regions = runtime.sprite_regions.get(tag);
    let shader = runtime.shaders.get(tag).cloned();
    let descendant_reflection = runtime
        .descendant_reflections
        .get(tag)
        .copied()
        .unwrap_or(false);
    let mut slots = definition
        .slots
        .iter()
        .filter_map(|slot| {
            let applied_binding = playback
                .latched_targets
                .get(slot)
                .filter(|target| target.sprite_applied);
            let (sprite, skin_transform, bound_region, bound_metrics) =
                if let Some(target) = applied_binding {
                    let binding = target.bound_sprite.as_ref()?;
                    (
                        binding.sprite.clone(),
                        binding.skin_transform.clone(),
                        binding.region.clone(),
                        Some(binding.metrics),
                    )
                } else {
                    // Synthetic state constructed by native-unit regressions
                    // has not run EntityTarget. Production scenes initialize
                    // every slot as applied-null and never enter this branch.
                    let (sprite, skin_transform) = animation_slot_attachment(
                        definition,
                        playback,
                        runtime.skins.get(tag).map(String::as_str),
                        slot,
                    )?;
                    let bound_region = sprite_regions
                        .and_then(|regions| regions.get(&sprite))
                        .cloned()?;
                    let bound_metrics =
                        sprite_metrics.and_then(|metrics| metrics.get(&sprite).copied());
                    (sprite, skin_transform, bound_region, bound_metrics)
                };
            let mut compatibility = animation_node_transform(definition, playback, slot, root)?;
            let mut transform = animation_node_world_affine(
                definition,
                playback,
                slot,
                root_affine,
                descendant_reflection,
            )?;
            if let Some(skin_transform) = skin_transform {
                transform = transform.then_skin_attachment(&skin_transform);
                compatibility.scale_x *= skin_transform.scale_x;
                compatibility.scale_y *= skin_transform.scale_y;
                compatibility.angle -= skin_transform.angle;
            }
            if let Some(center) = sprite_component_custom_centering(
                bound_metrics,
                sprite_geometry.and_then(|geometry| geometry.get(&sprite).copied()),
                Some(&bound_region),
            ) {
                transform = transform.compose(center);
            }
            let alpha = animation_target_sample(definition, playback, slot, |target| {
                !target.alpha.is_empty()
            })
            .map(|(target, time)| sample_float(&target.alpha, time, 1.0))
            .or_else(|| {
                playback
                    .latched_targets
                    .get(slot)
                    .map(|target| target.alpha)
            })
            .unwrap_or(1.0);
            let z_order = animation_target_sample(definition, playback, slot, |target| {
                !target.z_order.is_empty()
            })
            .and_then(|(target, time)| sample_discrete(&target.z_order, time))
            .or_else(|| {
                playback
                    .latched_targets
                    .get(slot)
                    .map(|target| target.z_order)
            })
            .unwrap_or(0);
            Some((
                z_order,
                sprite,
                transform,
                compatibility,
                alpha,
                bound_region,
            ))
        })
        .collect::<Vec<_>>();
    // Purple's animation component treats larger zOrder values as farther
    // back. Comic data makes the contract unambiguous: the opaque panel
    // background is z=14, character/details are z=5..13 and the border is
    // z=1..4. Software alpha composition must therefore visit z in descending
    // order so foreground slots are not hidden by their background.
    slots.sort_by_key(|slot| std::cmp::Reverse(slot.0));
    slots
        .into_iter()
        .map(
            |(_, sprite, transform, compatibility, alpha, bound_region)| {
                RenderCommand {
                    order: 0,
                    sprite,
                    texture: None,
                    texture_scale: 1.0,
                    masked_texture_binding: None,
                    bound_region: Some(bound_region.into()),
                    bound_composite: None,
                    shader: shader.clone(),
                    clip_holes: Vec::new(),
                    dirt: None,
                    x: transform.x,
                    y: transform.y,
                    state: RenderState {
                        scale_x: compatibility.scale_x,
                        scale_y: compatibility.scale_y,
                        angle: compatibility.angle,
                        matrix: Some([transform.m00, transform.m01, transform.m10, transform.m11]),
                        // SpriteComponentCustom keeps SpriteComponent's
                        // atlas-pivot vertices. Its derived draw member has
                        // already post-composed `pivot - size/2` above, so
                        // the two operations cancel to a centred raw quad.
                        sprite_pivot: None,
                        alpha,
                        ..RenderState::default()
                    },
                    world_space: true,
                }
            },
        )
        .collect()
}

/// Local matrix appended by `SpriteComponentCustom::draw` (`sub_100095A4C`).
///
/// The base SpriteComponent's default Anchor `{4,3}` builds vertices around
/// the signed SPRT pivot. The derived component then appends one translation
/// of `pivot - size * 0.5` before submitting those vertices. Keep both native
/// stages instead of replacing the atlas pivot with a host-side half-size:
/// under rotation/shear, the recovered `FMADD` and matrix-composition
/// roundings are observable at the final corners.
fn sprite_component_custom_centering(
    metrics: Option<NativeSpriteMetrics>,
    geometry: Option<SpriteGeometry>,
    region: Option<&SpriteCatalogRegion>,
) -> Option<AnimationAffine> {
    let (width, height, pivot_x, pivot_y) = if let Some(metrics) = metrics {
        (
            metrics.width as f32,
            metrics.height as f32,
            metrics.pivot_x as f32,
            metrics.pivot_y as f32,
        )
    } else if let Some(region) = region {
        let sprite = &region.sprite;
        (
            f32::from(sprite.width),
            f32::from(sprite.height),
            f32::from(sprite.pivot_x),
            f32::from(sprite.pivot_y),
        )
    } else {
        let geometry = geometry?;
        // SpriteGeometry stores the native pivot-relative rectangle:
        // min=-pivot and max=size-pivot.
        (
            geometry.width() as f32,
            geometry.height() as f32,
            -geometry.min_x as f32,
            -geometry.min_y as f32,
        )
    };
    Some(AnimationAffine {
        m00: 1.0,
        m01: 0.0,
        m10: 0.0,
        m11: 1.0,
        x: f64::from((-width).mul_add(0.5_f32, pivot_x)),
        y: f64::from((-height).mul_add(0.5_f32, pivot_y)),
    })
}
