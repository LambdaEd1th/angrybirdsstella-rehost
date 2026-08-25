//! RenderObjectData fields consumed by the two native scene draw members.

use crate::*;

/// The native z tree stores object names and resolves a live RenderObjectData
/// pointer when it reaches each vector entry. Keep only the fields consumed by
/// rendering; cloning the complete Rust physics body also copied fixture
/// arrays, material filters and solver state every frame.
#[derive(Debug, Clone)]
pub(crate) struct SceneDrawObject {
    pub(crate) sprite: String,
    pub(crate) sprite_bound: bool,
    pub(crate) sprite_region: Option<SpriteCatalogRegion>,
    pub(crate) composite_sprite: Option<Vec<BoundCompositePart>>,
    pub(crate) texture: Option<String>,
    pub(crate) texture_binding: Option<MaskedTextureBinding>,
    pub(crate) texture_scale: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) has_physics_body: bool,
    pub(crate) display_interpolation_velocities: [DisplayInterpolationVelocity; 2],
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
    pub(crate) sprite_rotation: f64,
    pub(crate) pivot_offset_x: f64,
    pub(crate) pivot_offset_y: f64,
    pub(crate) alpha: f64,
    pub(crate) visible: bool,
    pub(crate) flash_animation: bool,
    pub(crate) horizontal_flip: bool,
    pub(crate) sensor_type: i32,
    pub(crate) collision_is_circle: bool,
    pub(crate) decoration: Option<ObjectDecoration>,
    pub(crate) ray: Option<DrawablePolygonState>,
    pub(crate) dirt: Option<DirtComponent>,
    pub(crate) dirt_holes: Vec<DirtHole>,
}

impl From<&SceneObject> for SceneDrawObject {
    fn from(object: &SceneObject) -> Self {
        Self {
            sprite: object.sprite.clone(),
            sprite_bound: object.sprite_bound,
            sprite_region: object.sprite_region.clone(),
            composite_sprite: object.composite_sprite.clone(),
            texture: object.texture.clone(),
            texture_binding: object.texture_binding.clone(),
            texture_scale: object.texture_scale,
            x: object.render_x,
            y: object.render_y,
            has_physics_body: object.has_physics_body(),
            display_interpolation_velocities: object.display_interpolation_velocities,
            scale_x: object.scale_x,
            scale_y: object.scale_y,
            angle: object.render_angle,
            sprite_rotation: object.sprite_rotation,
            pivot_offset_x: object.pivot_offset_x,
            pivot_offset_y: object.pivot_offset_y,
            alpha: object.alpha,
            visible: object.visible,
            flash_animation: object.flash_animation,
            horizontal_flip: object.horizontal_flip,
            sensor_type: object.sensor_type,
            collision_is_circle: matches!(object.collision_shape, CollisionShape::Circle { .. }),
            decoration: object.decoration.clone(),
            ray: object.ray.clone(),
            dirt: object.dirt.clone(),
            dirt_holes: object.dirt_holes.clone(),
        }
    }
}
