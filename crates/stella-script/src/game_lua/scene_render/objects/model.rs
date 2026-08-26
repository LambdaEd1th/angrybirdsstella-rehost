//! RenderObjectData fields consumed by the two native scene draw members.

use std::sync::Arc;

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
    pub(crate) texture: Option<Arc<SpriteTextureSubmission>>,
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
}

/// Fields consumed before the pre callback is entered. Purple has not yet
/// installed this object's GL transform at that point; the remaining visual
/// state already lives in `SceneDrawObject` for the later draw/post phase.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneCallbackObject {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) native_shape_width: f64,
    pub(crate) native_shape_height: f64,
    pub(crate) is_water: bool,
    pub(crate) horizontal_flip: bool,
    pub(crate) collision_is_circle: bool,
    pub(crate) trajectory_anchor: bool,
}

/// One retained RenderObjectData visit after the native name-map lookup.
/// Purple keeps this pointer in `x21` and reads its Lua holders and visual
/// members directly. The deferred host snapshots the ordinary draw fields at
/// the same lookup so callback-free objects do not search the scene tree a
/// second time.
#[derive(Debug, Clone)]
pub(crate) struct SceneDrawVisit {
    pub(crate) callback_slot: usize,
    pub(crate) callback: SceneCallbackObject,
    pub(crate) initial_draw_object: SceneDrawObject,
}

impl From<&SceneObject> for SceneCallbackObject {
    fn from(object: &SceneObject) -> Self {
        Self {
            x: object.render_x,
            y: object.render_y,
            native_shape_width: object.native_shape_width,
            native_shape_height: object.native_shape_height,
            is_water: object.is_water,
            horizontal_flip: object.horizontal_flip,
            collision_is_circle: matches!(object.collision_shape, CollisionShape::Circle { .. }),
            // 0x10004BF28..0x10004BF9C inserts both trajectory buffers
            // immediately before the first +0x140 controllable or +0x148
            // level-goal object, then latches that insertion for this draw.
            trajectory_anchor: object.controllable || object.level_goal,
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
        }
    }
}
