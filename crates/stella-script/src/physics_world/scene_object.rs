//! Native body/render state owned by Purple's scene-object bridge.

use std::sync::Arc;

use crate::*;

/// Immutable payload inserted into GameLua's name-to-DrawablePolygon tree by
/// `makeRay`. The source contour and position are copied at insertion time;
/// subsequent body transforms do not rewrite this record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawablePolygonState {
    pub(crate) vertices: Vec<(f64, f64)>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) color: [f64; 4],
}

/// The two 12-byte RenderObjectData pose slots at offsets `+0x00/+0x0c`.
/// Box2D remains authoritative for gameplay; these float32 copies exist only
/// for the native per-display-frame interpolation pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeInterpolationPose {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) angle: f32,
}

/// Rehost-side companion to Purple's two native pose slots. The shipped
/// BirdAnimation update derives a flying bird's root rotation from the
/// fixed-step b2Body velocity after the native pose interpolation has already
/// run. Retaining the matching velocity samples lets the 60 Hz renderer
/// submit that derived angle without changing the authoritative 30 Hz body or
/// the Lua-visible velocity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DisplayInterpolationVelocity {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl DisplayInterpolationVelocity {
    pub(crate) const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub(crate) fn new(x: f64, y: f64) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
        }
    }
}

impl NativeInterpolationPose {
    pub(crate) fn new(x: f64, y: f64, angle: f64) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            angle: angle as f32,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SceneObject {
    pub(crate) physics_creation_order: u64,
    pub(crate) body_allocation_slot: Option<u64>,
    /// Host slot for RenderObjectData+0x20/+0x158/+0x160. Purple reaches all
    /// three retained Lua holders through the RenderObjectData pointer already
    /// resolved by the scene name map; the draw loop must not do another name
    /// lookup for them.
    pub(crate) draw_callback_slot: usize,
    pub(crate) fixture_proxy_ids: Vec<Option<i32>>,
    /// The target ABI's old libstdc++ string is a COW pointer. Keep the live
    /// RenderObjectData name fields pointer-cheap and materialize an owned Rust
    /// string only at the deferred wgpu command boundary.
    pub(crate) sprite: Arc<str>,
    /// Resource pointer retained when the sprite was assigned. Purple keeps
    /// these as retained AtlasSprite/CompoSprite pointers rather than copying
    /// the underlying resource graph into each render snapshot, so the Rust
    /// bridge mirrors that ownership with `Arc`. Both fields are empty when
    /// native lookup returned null; that must remain missing even if a
    /// resource with the same name loads later.
    pub(crate) sprite_bound: bool,
    pub(crate) sprite_region: Option<Arc<SpriteCatalogRegion>>,
    pub(crate) composite_sprite: Option<Arc<Vec<BoundCompositePart>>>,
    /// Retained fill-image pointer and its masked-draw inputs. Purple stores
    /// the image pointer on RenderObjectData and reuses it on every draw.
    pub(crate) texture: Option<Arc<SpriteTextureSubmission>>,
    pub(crate) texture_scale: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// RenderObjectData `+0xA4/+0xA8/+0xAC`. These are interpolated from the
    /// two native pose slots after the fixed Box2D loop and are the values
    /// consumed by both ordinary and Flash scene submission.
    pub(crate) render_x: f64,
    pub(crate) render_y: f64,
    pub(crate) render_angle: f64,
    pub(crate) interpolation_poses: [NativeInterpolationPose; 2],
    pub(crate) display_interpolation_velocities: [DisplayInterpolationVelocity; 2],
    // Purple keeps the b2Sweep centre separately from b2Transform::p.
    pub(crate) sweep_center_x: f32,
    pub(crate) sweep_center_y: f32,
    pub(crate) collision_shape: CollisionShape,
    pub(crate) native_shape_width: f64,
    pub(crate) native_shape_height: f64,
    pub(crate) native_shape_radius: f64,
    pub(crate) dynamic_body: bool,
    pub(crate) kinematic_body: bool,
    pub(crate) body_mass: f32,
    pub(crate) inverse_mass: f64,
    /// b2Body's cached fixture aggregate. Box2D computes this only from
    /// CreateFixture/DestroyFixture/ResetMassData, then every solver pass
    /// reads the stored local centre and inertia directly.
    pub(crate) fixture_mass_data: (f32, (f32, f32), f32),
    pub(crate) moment_of_inertia: Option<f64>,
    pub(crate) density: f64,
    pub(crate) fixture_densities: Vec<f64>,
    pub(crate) z_order: f64,
    /// RenderObjectData+0xBC/+0xC0: live visual scale consumed by rendering
    /// and overwritten by the collision bounce pass.
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    /// RenderObjectData+0xCC/+0xD0: persistent visual scale used as the base
    /// for bounce animation. Ordinary scale setters write both scale pairs.
    pub(crate) base_scale_x: f64,
    pub(crate) base_scale_y: f64,
    /// Collision-fixture scale retained by the Rust Box2D reconstruction;
    /// this is independent of both native visual-scale pairs above.
    pub(crate) physics_scale_x: f64,
    pub(crate) physics_scale_y: f64,
    pub(crate) angle: f64,
    pub(crate) sprite_rotation: f64,
    pub(crate) pivot_offset_x: f64,
    pub(crate) pivot_offset_y: f64,
    pub(crate) alpha: f64,
    pub(crate) visible: bool,
    pub(crate) flash_animation: bool,
    pub(crate) velocity_x: f64,
    pub(crate) velocity_y: f64,
    pub(crate) angular_velocity: f64,
    pub(crate) force_x: f64,
    pub(crate) force_y: f64,
    pub(crate) torque: f64,
    pub(crate) restitution: f64,
    pub(crate) friction: f64,
    pub(crate) fixture_restitutions: Vec<f64>,
    pub(crate) fixture_frictions: Vec<f64>,
    pub(crate) linear_damping: f64,
    pub(crate) angular_damping: f64,
    pub(crate) gravity_scale: f64,
    pub(crate) gravity_category: i32,
    pub(crate) sensor_gravity_mask: i32,
    pub(crate) fixed_rotation: bool,
    pub(crate) bullet: bool,
    pub(crate) sensor: bool,
    pub(crate) active: bool,
    pub(crate) sleeping: bool,
    pub(crate) collision_enabled: bool,
    pub(crate) block_collision_enabled: bool,
    pub(crate) controllable: bool,
    pub(crate) ignores_score: bool,
    pub(crate) keep_orientation: bool,
    pub(crate) record_velocity: bool,
    pub(crate) revert_gravity: bool,
    pub(crate) revert_gravity_with_multiplier: bool,
    pub(crate) revert_gravity_force: f64,
    pub(crate) revert_gravity_max_velocity: f64,
    pub(crate) time_since_collision: f64,
    pub(crate) level_goal: bool,
    pub(crate) bounce_amplitude_multiplier: f64,
    pub(crate) bounce_frequency_multiplier: f64,
    pub(crate) bounce_active: bool,
    pub(crate) bounce_elapsed: f64,
    pub(crate) bounce_current_amplitude: f64,
    pub(crate) bounce_initial_amplitude: f64,
    pub(crate) graphics_flip_enabled: bool,
    pub(crate) not_collided: bool,
    pub(crate) ignore_motion: bool,
    pub(crate) disable_immovable_collisions: bool,
    pub(crate) sensor_definition: bool,
    /// RenderObjectData+0x139, written by ObjectParameter 8.
    pub(crate) horizontal_flip: bool,
    pub(crate) sensor_active: bool,
    pub(crate) sensor_type: i32,
    pub(crate) sensor_shape_type: i32,
    pub(crate) sensor_radius: f64,
    pub(crate) sensor_width: f64,
    pub(crate) sensor_height: f64,
    pub(crate) sensor_force_angle: f64,
    pub(crate) aiming_aid_collideable: bool,
    pub(crate) bubble: bool,
    pub(crate) collision_group: i32,
    pub(crate) sensor_minimum_force: f64,
    pub(crate) sensor_maximum_force: f64,
    /// RenderObjectData+0x18: native wood/stone/glass enum written by
    /// `setMaterial`. This is independent of Lua's string-valued material
    /// attribute used by the game-side contact filter.
    pub(crate) native_material: i32,
    pub(crate) material: String,
    pub(crate) collision_materials: Vec<String>,
    pub(crate) water_density: f64,
    pub(crate) is_water: bool,
    pub(crate) decoration: Option<Arc<ObjectDecoration>>,
    pub(crate) ray: Option<Arc<DrawablePolygonState>>,
    pub(crate) dirt: Option<Arc<DirtComponent>>,
    pub(crate) dirt_holes: Arc<Vec<DirtHole>>,
    pub(crate) motion_started: bool,
    pub(crate) sleep_time: f64,
    pub(crate) native_was_awake: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ObjectDecoration {
    pub(crate) amount: i32,
    pub(crate) sprite: String,
    pub(crate) angle_increment: f64,
    pub(crate) scale: f64,
}
