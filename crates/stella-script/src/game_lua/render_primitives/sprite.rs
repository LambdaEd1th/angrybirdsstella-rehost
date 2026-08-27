use crate::*;
use std::sync::Arc;

pub(crate) fn native_direct_sprite_command(
    sprite: SharedSpriteName,
    bound_region: Arc<SpriteCatalogRegion>,
    shader: Option<SpriteShader>,
    placement: NativeSpritePlacement,
    parent: RenderState,
) -> RenderCommand {
    // sub_10006C838 constructs the complete T * R * Scale matrix supplied to
    // AtlasSprite::draw.  sub_100467BE8 applies that matrix directly to the
    // four atlas vertices; the GL_Context's live transform is not multiplied
    // into it.  The live state still supplies color/alpha and clipping.  This
    // distinction matters for post-draw sprites such as pig pupils: composing
    // the object callback state here would scale them twice and cancel the
    // rotation of a horizontally flipped pig.
    let origin = [f64::from(placement.x as f32), f64::from(placement.y as f32)];
    let (sine, cosine) = (placement.angle as f32).sin_cos();
    let scale_x = placement.scale_x as f32;
    let scale_y = placement.scale_y as f32;
    let matrix = [
        f64::from(cosine * scale_x),
        f64::from(-sine * scale_y),
        f64::from(sine * scale_x),
        f64::from(cosine * scale_y),
    ];
    RenderCommand {
        order: 0,
        sprite,
        texture: None,
        bound_region: Some(bound_region),
        bound_composite: None,
        geometry: None,
        shader: shader.map(Arc::new),
        dirt: None,
        x: origin[0] as f32,
        y: origin[1] as f32,
        state: RenderState {
            matrix: Some(matrix),
            alpha: parent.alpha,
            clip_rect: parent.clip_rect,
            ..RenderState::default()
        }
        .into(),
        world_space: true,
    }
}
