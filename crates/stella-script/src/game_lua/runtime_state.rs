//! Aggregate `GameLua` runtime state recovered from `sub_10002C274`.

use crate::*;
use mlua::{Function, Table};
use std::collections::{BTreeMap, BTreeSet};
use stella_assets::ka3d::CompositePart;

/// GameApp's contiguous camera-input fields at `+0x4FC..+0x51C`. Purple's
/// separate process-global two-touch baseline lives beside `host_input`.
#[derive(Debug)]
pub(crate) struct NativeInputZoom {
    pub(crate) current: f32,
    pub(crate) previous: f32,
    pub(crate) smooth_start: f32,
    pub(crate) smooth_target: f32,
    pub(crate) smooth_elapsed: f32,
    pub(crate) smooth_duration: f32,
    pub(crate) wheel_pending: bool,
}

impl Default for NativeInputZoom {
    fn default() -> Self {
        Self {
            current: 1.0,
            previous: 1.0,
            smooth_start: 0.0,
            smooth_target: 0.0,
            smooth_elapsed: -1.0,
            smooth_duration: -1.0,
            wheel_pending: false,
        }
    }
}

/// Retained broad-phase payload owned by one native b2Body's fixture list.
/// Purple stores the tight/swept AABB and proxy id beside each fixture proxy;
/// the rehost keeps ids on `SceneObject` for fixture-list lifecycle, while
/// co-locating the remaining proxy scalars under one body lookup. This avoids
/// manufacturing `(String, fixture)` tree keys during every solver step.
#[derive(Debug)]
pub(crate) struct NativeBodyProxyState {
    pub(crate) tight_aabbs: Vec<NativeAabb>,
    pub(crate) fat_aabbs: Vec<NativeAabb>,
    pub(crate) position: (f32, f32),
}

#[derive(Debug)]
pub(crate) struct RenderBridge {
    pub(crate) state: RenderState,
    pub(crate) background_color: [u8; 3],
    /// GameLua+0x250..+0x258. `setTheme` stores the raw float32 sky color;
    /// the background ThemeManager draw applies it to the renderer later.
    pub(crate) theme_sky_color: [f32; 3],
    /// GameLua+0x25C..+0x264. Purple parses and retains the optional ground
    /// color (or zeroes it), but the 1.1.6 executable has no read of it.
    pub(crate) theme_ground_color: [f32; 3],
    /// Renderer vtable slots `+0xD8/+0xE0` report the live drawable extent.
    /// ThemeManager and GameLua query these values during update and draw;
    /// they are not fixed to the authored 1024-by-768 asset resolution.
    pub(crate) screen_width: u32,
    pub(crate) screen_height: u32,
    /// `gr::Context+0x310`, returned by virtual slot `+0x120`. Purple maps
    /// this native orientation enum through `{0, 90, 180, 270}` before
    /// exposing it to Lua in `sub_100051298`.
    pub(crate) device_orientation_index: u32,
    pub(crate) commands: Vec<RenderCommand>,
    pub(crate) text_commands: Vec<TextRenderCommand>,
    pub(crate) rect_commands: Vec<RectRenderCommand>,
    pub(crate) capture_commands: Vec<CaptureRenderCommand>,
    pub(crate) next_draw_order: u64,
    /// Monotonic stand-in for the intrusive Box2D world/body/contact/joint
    /// lists. Native creation inserts at each list head, so larger values are
    /// visited first by b2World::Solve and each body's edge traversal.
    pub(crate) next_physics_creation_order: u64,
    /// b2World's intrusive body list, indexed by the monotonically allocated
    /// creation token. Reverse iteration is the native head-to-tail order;
    /// unlike `scene`, this contains only records that own a b2Body.
    pub(crate) native_body_world_order: BTreeMap<u64, String>,
    /// Intrusive b2ContactEdge order retained per native body. Entries are
    /// appended in creation order and traversed in reverse because Box2D
    /// inserts every new edge at the list head.
    pub(crate) native_body_contact_edges: BTreeMap<u64, Vec<u64>>,
    /// Stable body endpoints retained by each live contact allocation.
    pub(crate) native_contact_body_orders: BTreeMap<u64, (u64, u64)>,
    /// Intrusive b2JointEdge order retained per native body.
    pub(crate) native_body_joint_edges: BTreeMap<u64, Vec<u64>>,
    /// Stable body endpoints retained by each live physical joint allocation.
    pub(crate) native_joint_body_orders: BTreeMap<u64, (u64, u64)>,
    /// Address-sized slot model for the native 0xC0-byte b2Body allocations.
    /// Unlike intrusive-list order, public QueryAABB results are sorted by
    /// `std::set<b2Body*>`, so a destroyed body's block-allocator slot must be
    /// reused before extending the allocation range.
    pub(crate) next_body_allocation_slot: u64,
    pub(crate) free_body_allocation_slots: Vec<u64>,
    pub(crate) dynamic_tree: NativeDynamicTree,
    pub(crate) composite_updates: BTreeMap<String, Vec<CompositePart>>,
    pub(crate) scene: BTreeMap<String, SceneObject>,
    /// Identity of the Lua `objects.world` table that owns `scene`.
    ///
    /// Level restart replaces that table before rebuilding objects with the
    /// same names. Purple destroys the preceding GameScene/Box2D owner at
    /// that boundary; comparing only the live string keys would therefore
    /// retain stale render-index entries and draw the rebuilt level twice.
    pub(crate) object_world_identity: Option<usize>,
    /// Persistent GameLua+0x310 tree. Unlike `scene`, removal never prunes
    /// empty z/SpriteSheet nodes, which remains observable by DrawCalls.
    pub(crate) scene_render_index: NativeSceneRenderIndex,
    pub(crate) z_order_min: f64,
    pub(crate) z_order_max: f64,
    pub(crate) top_left_x: f64,
    pub(crate) top_left_y: f64,
    pub(crate) world_scale: f64,
    pub(crate) max_world_scale: f64,
    pub(crate) camera_limit: f64,
    pub(crate) level_limits: [f64; 4],
    /// Signed framebuffer-space bounds at GameLua `+0x618..+0x624` used by
    /// infinite `ParticleData` records after every integration step.
    pub(crate) particle_wrap_limits: [i32; 4],
    /// GameLua `+0x554`; Purple loads and multiplies this field through `S`
    /// registers before publishing either frame delta to Lua.
    pub(crate) delta_time_multiplier: f32,
    pub(crate) physics_simulation_scale: f64,
    /// GameLua `+0x51c`; the fixed-step comparison and repeated subtraction
    /// are both single precision in `sub_10005E898`.
    pub(crate) physics_accumulator: f32,
    /// GameLua `+0x23c`; each retained tail step flips this zero/one selector
    /// before copying every awake body's transform into one of the two
    /// RenderObjectData interpolation slots.
    pub(crate) physics_interpolation_slot: usize,
    pub(crate) vertex_buffer: Vec<(f64, f64)>,
    pub(crate) active_contacts: BTreeMap<ContactKey, bool>,
    pub(crate) contact_creation_order: BTreeMap<ContactKey, u64>,
    /// `b2World::m_contactList`, indexed by the allocator-wide creation
    /// sequence. New contacts are linked at the head, so reverse iteration
    /// reproduces native `m_next` traversal without sorting string keys.
    pub(crate) native_contact_world_order: BTreeMap<u64, ContactKey>,
    pub(crate) broad_phase_contacts: BTreeSet<ContactKey>,
    /// Native `b2Contact::e_filterFlag` (`0x8`). Joint topology changes mark
    /// existing contacts, but `ContactManager::Collide` does not consume the
    /// flag until at least one non-static endpoint is awake.
    pub(crate) contact_filter_dirty: BTreeSet<ContactKey>,
    pub(crate) body_proxy_states: BTreeMap<String, NativeBodyProxyState>,
    pub(crate) moved_proxy_ids: BTreeSet<i32>,
    /// Every currently touching non-sensor manifold after the one native
    /// ContactManager::Collide pass. Island assembly selects a subset of
    /// these without rerunning the narrow phase.
    pub(crate) contact_manifolds: BTreeMap<ContactKey, ContactManifold>,
    pub(crate) velocity_contacts: BTreeMap<ContactKey, ContactManifold>,
    /// Per-island b2ContactSolver constraint impulses. Purple keeps these
    /// separate from the contact manifold until StoreImpulses runs after the
    /// final velocity iteration.
    pub(crate) solver_contact_impulses: BTreeMap<ContactKey, CachedContactImpulse>,
    /// Temporary b2ContactSolver body-index array. It exists only while one
    /// island contact pass runs and is committed before joints run again.
    pub(crate) contact_velocity_cache: Option<NativeContactVelocityCache>,
    pub(crate) contact_impulses: BTreeMap<ContactKey, CachedContactImpulse>,
    pub(crate) contact_velocity_bias: BTreeMap<ContactKey, [f32; 2]>,
    pub(crate) position_contacts: BTreeMap<ContactKey, PositionContactConstraint>,
    /// Awake islands assembled for the current fixed step. Box2D solves and
    /// sleeps each island independently rather than interleaving the whole
    /// world's constraints.
    pub(crate) solver_islands: Vec<SolverIsland>,
    /// Non-static island-flagged bodies in b2World list order after the most
    /// recent discrete assembly. `Solve` walks this same list once after all
    /// islands to synchronize fixtures, without sorting the island arrays.
    pub(crate) solver_synchronized_bodies: Vec<String>,
    /// Reusable owner for the current island's stable `b2Joint*` equivalent.
    /// It is empty outside `b2Island::Solve`, retaining only vector capacity.
    pub(crate) joint_constraint_scratch: NativeIslandJointConstraints,
    pub(crate) native_sensor_overlaps: BTreeMap<String, BTreeSet<String>>,
    pub(crate) inside_gravity_objects: BTreeSet<String>,
    pub(crate) collision_velocities: BTreeMap<String, (f64, f64)>,
    pub(crate) joints: BTreeMap<String, PhysicsJoint>,
    /// Physical `b2Joint` records in world-list creation order. Metadata-only
    /// destruction links never enter this index.
    pub(crate) native_joint_world_order: BTreeMap<u64, String>,
    /// GameLua `+0x3F0/+0x3F8`: breakable joints are removed from the
    /// logical 48-byte `jointData` vector during BeginContact, but their
    /// Box2D joints stay alive until the frame-tail drain after Lua update
    /// and particle integration. Names are appended in predicate order and
    /// destroyed from the back, matching the native reverse walk.
    pub(crate) pending_native_joint_destructions: Vec<String>,
    /// RenderObjectData `+0x146/+0xD8`: type-five destruction links mark a
    /// target and count down before queuing it in Lua `deadBlocks`. They do
    /// not remove the native object directly.
    pub(crate) pending_object_destructions: BTreeMap<String, f64>,
    pub(crate) tracks: BTreeMap<String, PhysicsTrack>,
    /// Separately allocated 0x88-byte `Particles` owner constructed at
    /// `0x10002F6A8..0x10002F6C4` by `sub_10008E160`.
    pub(crate) particle_system: NativeParticles,
    /// Process-global random source consumed by the native Particles member.
    pub(crate) particle_random: NativeParticleRandom,
    /// GameLua+0x199 gates only in-game particle update/draw modes 1 and 2.
    pub(crate) particles_enabled: bool,
    pub(crate) theme_background_layers: Vec<ThemeLayer>,
    pub(crate) theme_foreground_layers: Vec<ThemeLayer>,
    /// ThemeManager+0xC0/+0xC8: distinct derived particle systems for the
    /// background and foreground layer passes.
    pub(crate) theme_background_particles: NativeThemeParticles,
    pub(crate) theme_foreground_particles: NativeThemeParticles,
    pub(crate) theme_sprites: NativeThemeSprites,
    pub(crate) theme_offset_y: f64,
    pub(crate) theme_camera: ThemeCameraReference,
    pub(crate) physics_enabled: bool,
    /// Native `b2World::e_locked`: true only while `b2World::Step` is inside
    /// contact refresh/island/TOI work. Mutating Box2D members such as
    /// `b2Body::SetType` reject callback re-entry while this flag is set.
    pub(crate) physics_world_locked: bool,
    pub(crate) physics_lock_count: u32,
    pub(crate) physics_locks: BTreeMap<String, u32>,
    /// GameLua+0x2C8/+0x2CC/+0x2D0 retain the rock, wood and light rolling
    /// AudioClipInstance handles even after stop-by-resource-name.
    pub(crate) rolling_audio_handles: [i64; 3],
    pub(crate) world_gravity_x: f64,
    pub(crate) world_gravity_y: f64,
    pub(crate) gravity_force_multiplier: f64,
    pub(crate) water_force_multiplier: f64,
    pub(crate) object_water_drag: f64,
    pub(crate) bird_water_drag: f64,
    pub(crate) additional_bird_gravity: f64,
    pub(crate) water_color: [f64; 4],
    pub(crate) deterministic_physics: bool,
    pub(crate) game_world_scale: f64,
    pub(crate) game_on: bool,
    pub(crate) game_rendering_disabled: bool,
    pub(crate) exit_requested: bool,
    /// GameLua+0x6AC, refreshed from the Lua `g_safeToQuit` value once per
    /// native frame and returned through GameApp's virtual slot +0xB8.
    pub(crate) safe_to_quit: bool,
    /// ThemeManager+0xA8. Resolution changes snapshot the corrected end
    /// camera's `sx` before invoking Lua's `resolutionChanged` callback.
    pub(crate) resolution_camera_scale: f32,
    pub(crate) starting_camera_value: bool,
    pub(crate) requested_video: Option<String>,
    pub(crate) requested_url: Option<String>,
    pub(crate) requested_app_store_product: Option<(String, u32)>,
    pub(crate) screenshot_share_requests: Vec<ScreenshotShareRequest>,
    pub(crate) smooth_zooming: bool,
    pub(crate) input_zoom: NativeInputZoom,
    pub(crate) accelerometer_active: bool,
    /// Platform accelerometer pair returned in S0/S1 by `sub_100532CC0`.
    /// Desktop hosts leave this at zero; native integrations can publish a
    /// sample without changing the recovered GameLua filter.
    pub(crate) accelerometer_sample: [f32; 2],
    /// GameLua+0x2A4/+0x2A8, reset by `setAccelerometerActive` and filtered
    /// once per native frame before both ThemeManager passes.
    pub(crate) accelerometer_filtered: [f32; 2],
    pub(crate) editing: bool,
    pub(crate) notification_callback: Option<String>,
    pub(crate) aiming_aid_enabled: bool,
    pub(crate) aiming_aid_sprite: String,
    /// GameLua+0x4F8/+0x4FC/+0x500. `loadLevelImpl` snapshots these three
    /// `worldAttributes` values once; only `objects.currentTimeStep` remains
    /// a live Lua input to the native trajectory predictor.
    pub(crate) simulation_iterations: i32,
    pub(crate) simulation_time_step_multiplier: f32,
    pub(crate) simulation_store_points_sampler: i32,
    pub(crate) aim_stream_active: bool,
    /// AimStream+0x40/+0x48. `loadLevelImpl` snapshots the two
    /// worldAttributes values once; later Lua table mutations do not alter
    /// the already configured native stream.
    pub(crate) aim_stream_spawn_time: f32,
    pub(crate) aim_stream_speed: f32,
    pub(crate) aim_stream_spawn_timer: f32,
    pub(crate) aim_stream_particles: Vec<NativeAimParticle>,
    pub(crate) selected_simulation_bird: Option<String>,
    /// GameLua+0x588 selects one of the two 0x38-byte flight-trail records.
    pub(crate) trajectory_stream_index: usize,
    pub(crate) trajectory_streams: [NativeTrajectoryBuffer; 2],
    /// The raw simulation vector at GameLua+0x590, exposed verbatim through
    /// getSimulationTrajectoryPoints.
    pub(crate) trajectory_points: Vec<(f64, f64)>,
    /// AimStream+0x28.  updateBirdTrajectoryTable only replaces it when at
    /// least four raw samples exist and duplicates both endpoints.
    pub(crate) aim_stream_control_points: Vec<(f64, f64)>,
}

#[derive(Clone)]
pub(crate) struct DrawCallbackRecord {
    /// RenderObjectData+0x20. Purple retains the exact Lua object table in a
    /// registry reference when the native record is constructed.
    pub(crate) object: Table,
    /// RenderObjectData+0x158/+0x160. The two callback holders live in the same
    /// native record; one tree lookup therefore resolves all three references.
    pub(crate) pre: Option<Function>,
    pub(crate) post: Option<Function>,
}

#[derive(Default)]
pub(crate) struct DrawCallbacks {
    /// Constructor name index used by infrequent mutation/removal members.
    /// The hot draw path follows the slot retained by `SceneObject`, matching
    /// Purple's direct `RenderObjectData*` fields instead of searching this
    /// tree for every visible object.
    pub(crate) records: BTreeMap<String, usize>,
    slots: Vec<Option<DrawCallbackRecord>>,
    pub(crate) object_world_identity: Option<usize>,
}

impl DrawCallbacks {
    pub(crate) fn clear_records(&mut self) {
        self.records.clear();
        self.slots.clear();
    }

    /// Install the three retained Lua holders that live on one native
    /// RenderObjectData allocation. A same-name constructor replaces that
    /// allocation in the native name map while old render leaves continue to
    /// resolve the new pointer, so reuse its stable host slot as well.
    pub(crate) fn insert_record(&mut self, name: String, record: DrawCallbackRecord) -> usize {
        if let Some(&slot) = self.records.get(&name) {
            self.slots[slot] = Some(record);
            return slot;
        }
        let slot = self.slots.len();
        self.slots.push(Some(record));
        self.records.insert(name, slot);
        slot
    }

    pub(crate) fn remove_record(&mut self, name: &str) -> Option<DrawCallbackRecord> {
        let slot = self.records.remove(name)?;
        self.slots.get_mut(slot)?.take()
    }

    pub(crate) fn record(&self, slot: usize) -> Option<&DrawCallbackRecord> {
        self.slots.get(slot)?.as_ref()
    }

    pub(crate) fn record_mut(&mut self, slot: usize) -> Option<&mut DrawCallbackRecord> {
        self.slots.get_mut(slot)?.as_mut()
    }
}

impl Default for RenderBridge {
    fn default() -> Self {
        Self {
            state: RenderState::default(),
            // GameLua::GameLua stores packed 0xFFFF_FFFF at +0x238 before
            // the boot scripts select their theme background.
            background_color: [0xff; 3],
            theme_sky_color: [0.0; 3],
            theme_ground_color: [0.0; 3],
            screen_width: 1024,
            screen_height: 768,
            // The target declares the first landscape orientation as its
            // canonical startup orientation.
            device_orientation_index: 1,
            commands: Vec::new(),
            text_commands: Vec::new(),
            rect_commands: Vec::new(),
            capture_commands: Vec::new(),
            next_draw_order: 0,
            next_physics_creation_order: 0,
            native_body_world_order: BTreeMap::new(),
            native_body_contact_edges: BTreeMap::new(),
            native_contact_body_orders: BTreeMap::new(),
            native_body_joint_edges: BTreeMap::new(),
            native_joint_body_orders: BTreeMap::new(),
            next_body_allocation_slot: 0,
            free_body_allocation_slots: Vec::new(),
            dynamic_tree: NativeDynamicTree::default(),
            composite_updates: BTreeMap::new(),
            scene: BTreeMap::new(),
            object_world_identity: None,
            scene_render_index: NativeSceneRenderIndex::default(),
            z_order_min: f64::NEG_INFINITY,
            z_order_max: f64::INFINITY,
            top_left_x: 0.0,
            top_left_y: 0.0,
            world_scale: 1.0,
            max_world_scale: 1.0,
            camera_limit: 0.0,
            level_limits: [0.0; 4],
            particle_wrap_limits: [0; 4],
            delta_time_multiplier: 1.0,
            // GameLua::GameLua writes 1.0f to +0x50c at 0x10002c55c.
            // Shipped gamelogic later replaces it with physicsToWorld (20)
            // from initParams, so keep the constructor and boot phases
            // distinct just as Purple does.
            physics_simulation_scale: 1.0,
            physics_accumulator: 0.0,
            physics_interpolation_slot: 0,
            vertex_buffer: Vec::new(),
            active_contacts: BTreeMap::new(),
            contact_creation_order: BTreeMap::new(),
            native_contact_world_order: BTreeMap::new(),
            broad_phase_contacts: BTreeSet::new(),
            contact_filter_dirty: BTreeSet::new(),
            body_proxy_states: BTreeMap::new(),
            moved_proxy_ids: BTreeSet::new(),
            contact_manifolds: BTreeMap::new(),
            velocity_contacts: BTreeMap::new(),
            solver_contact_impulses: BTreeMap::new(),
            contact_velocity_cache: None,
            contact_impulses: BTreeMap::new(),
            contact_velocity_bias: BTreeMap::new(),
            position_contacts: BTreeMap::new(),
            solver_islands: Vec::new(),
            solver_synchronized_bodies: Vec::new(),
            joint_constraint_scratch: NativeIslandJointConstraints::default(),
            native_sensor_overlaps: BTreeMap::new(),
            inside_gravity_objects: BTreeSet::new(),
            collision_velocities: BTreeMap::new(),
            joints: BTreeMap::new(),
            native_joint_world_order: BTreeMap::new(),
            pending_native_joint_destructions: Vec::new(),
            pending_object_destructions: BTreeMap::new(),
            tracks: BTreeMap::new(),
            particle_system: NativeParticles::default(),
            particle_random: NativeParticleRandom::default(),
            particles_enabled: true,
            theme_background_layers: Vec::new(),
            theme_foreground_layers: Vec::new(),
            theme_background_particles: NativeThemeParticles::default(),
            theme_foreground_particles: NativeThemeParticles::default(),
            theme_sprites: NativeThemeSprites::default(),
            theme_offset_y: 0.0,
            theme_camera: ThemeCameraReference::default(),
            // GameLua::GameLua (`sub_10002C274`) writes one to +0x6A8.
            // The matching unnamed lock is released by the shipped startup
            // scripts' first `setPhysicsEnabled(true)` call.
            physics_enabled: false,
            physics_world_locked: false,
            physics_lock_count: 1,
            physics_locks: BTreeMap::from([(String::new(), 1)]),
            rolling_audio_handles: [0; 3],
            world_gravity_x: 0.0,
            world_gravity_y: 0.0,
            // GameLua::GameLua initializes +0x530/+0x534 to 4.0f/2.0f.
            gravity_force_multiplier: 4.0,
            water_force_multiplier: 2.0,
            object_water_drag: 1.0,
            bird_water_drag: f64::from(0.4_f32),
            // Constructor store 0xBF80_0000 at byte offset +0x224.
            additional_bird_gravity: -1.0,
            water_color: [1.0; 4],
            deterministic_physics: false,
            game_world_scale: 1.0,
            game_on: true,
            game_rendering_disabled: false,
            exit_requested: false,
            safe_to_quit: false,
            resolution_camera_scale: 1.0,
            starting_camera_value: false,
            requested_video: None,
            requested_url: None,
            requested_app_store_product: None,
            screenshot_share_requests: Vec::new(),
            // GameApp::GameApp stores one at +0x514 after loading the native
            // game configuration; +0x50C/+0x510 begin at -1.0f.
            smooth_zooming: true,
            input_zoom: NativeInputZoom::default(),
            accelerometer_active: false,
            accelerometer_sample: [0.0; 2],
            accelerometer_filtered: [0.0; 2],
            editing: false,
            notification_callback: None,
            aiming_aid_enabled: false,
            aiming_aid_sprite: String::new(),
            // Native gameplay never consumes +0x4F8..+0x500 before the
            // first loadLevelImpl write. Keep the host's pre-level state
            // deterministic; every playable level replaces these values.
            simulation_iterations: 0,
            simulation_time_step_multiplier: 0.0,
            simulation_store_points_sampler: 0,
            aim_stream_active: false,
            // AimStream's constructor at sub_100007E1C stores 0.6f/4.0f.
            aim_stream_spawn_time: 0.6_f32,
            aim_stream_speed: 4.0_f32,
            aim_stream_spawn_timer: 0.0,
            aim_stream_particles: Vec::new(),
            selected_simulation_bird: None,
            trajectory_stream_index: 0,
            trajectory_streams: [
                NativeTrajectoryBuffer::default(),
                NativeTrajectoryBuffer::default(),
            ],
            trajectory_points: Vec::new(),
            aim_stream_control_points: Vec::new(),
        }
    }
}
