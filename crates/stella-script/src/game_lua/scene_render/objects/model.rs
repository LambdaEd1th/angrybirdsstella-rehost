//! RenderObjectData fields consumed by the two native scene draw members.

use std::sync::Arc;

use super::pivot::native_scene_callback_pivot;
use crate::*;

/// The native z tree stores object names and resolves a live RenderObjectData
/// pointer when it reaches each vector entry. Keep only the fields consumed by
/// rendering; cloning the complete Rust physics body also copied fixture
/// arrays, material filters and solver state every frame.
#[derive(Debug, Clone)]
pub(crate) struct SceneDrawObject {
    pub(crate) sprite: Arc<str>,
    pub(crate) sprite_bound: bool,
    /// Retained native-style resource pointers. `SceneDrawObject::from`
    /// snapshots scalar pose/state, but deliberately does not duplicate the
    /// atlas/composite resource graph before command submission.
    pub(crate) sprite_region: Option<Arc<SpriteCatalogRegion>>,
    pub(crate) composite_sprite: Option<Arc<Vec<BoundCompositePart>>>,
    pub(crate) texture: Option<Arc<str>>,
    pub(crate) texture_binding: Option<Arc<MaskedTextureBinding>>,
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
    pub(crate) flash_animation: bool,
    pub(crate) horizontal_flip: bool,
    pub(crate) sensor_type: i32,
    pub(crate) collision_is_circle: bool,
    pub(crate) decoration: Option<Arc<ObjectDecoration>>,
    pub(crate) ray: Option<Arc<DrawablePolygonState>>,
    pub(crate) dirt: Option<Arc<DirtComponent>>,
    pub(crate) dirt_holes: Arc<Vec<DirtHole>>,
}

/// Scalar callback context copied from the live RenderObjectData record.
/// Purple keeps the record and its retained sprite pointer in place while a
/// callback runs; calculating the pivot before releasing the bridge lock
/// avoids deep-cloning those resource graphs just to install GL state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneCallbackObject {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) native_shape_width: f64,
    pub(crate) native_shape_height: f64,
    pub(crate) is_water: bool,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
    pub(crate) sprite_rotation: f64,
    pub(crate) pivot_offset_x: f64,
    pub(crate) pivot_offset_y: f64,
    pub(crate) pivot_x: f32,
    pub(crate) pivot_y: f32,
    pub(crate) alpha: f64,
    pub(crate) horizontal_flip: bool,
    pub(crate) sensor_type: i32,
    pub(crate) collision_is_circle: bool,
}

impl From<&SceneObject> for SceneCallbackObject {
    fn from(object: &SceneObject) -> Self {
        let (pivot_x, pivot_y) = native_scene_callback_pivot(
            object.composite_sprite.as_deref().map(Vec::as_slice),
            object.sprite_region.as_deref(),
            object.pivot_offset_x,
            object.pivot_offset_y,
        );
        Self {
            x: object.render_x,
            y: object.render_y,
            native_shape_width: object.native_shape_width,
            native_shape_height: object.native_shape_height,
            is_water: object.is_water,
            scale_x: object.scale_x,
            scale_y: object.scale_y,
            angle: object.render_angle,
            sprite_rotation: object.sprite_rotation,
            pivot_offset_x: object.pivot_offset_x,
            pivot_offset_y: object.pivot_offset_y,
            pivot_x,
            pivot_y,
            alpha: object.alpha,
            horizontal_flip: object.horizontal_flip,
            sensor_type: object.sensor_type,
            collision_is_circle: matches!(object.collision_shape, CollisionShape::Circle { .. }),
        }
    }
}

#[cfg(test)]
impl From<&SceneDrawObject> for SceneCallbackObject {
    fn from(object: &SceneDrawObject) -> Self {
        let (pivot_x, pivot_y) = object.callback_pivot();
        Self {
            x: object.x,
            y: object.y,
            // This conversion only supports callback-state unit tests. Water
            // submission snapshots the live SceneObject directly above.
            native_shape_width: 0.0,
            native_shape_height: 0.0,
            is_water: false,
            scale_x: object.scale_x,
            scale_y: object.scale_y,
            angle: object.angle,
            sprite_rotation: object.sprite_rotation,
            pivot_offset_x: object.pivot_offset_x,
            pivot_offset_y: object.pivot_offset_y,
            pivot_x,
            pivot_y,
            alpha: object.alpha,
            horizontal_flip: object.horizontal_flip,
            sensor_type: object.sensor_type,
            collision_is_circle: object.collision_is_circle,
        }
    }
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
