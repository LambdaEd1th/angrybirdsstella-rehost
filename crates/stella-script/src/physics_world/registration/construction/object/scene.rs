//! Native SceneObject/Box2D-side allocation and initialization.

use std::sync::{Arc, Mutex};

use crate::{RenderBridge, SceneObject};

use super::super::{ConstructorKind, PreparedConstruction};

pub(super) fn insert(render: &Arc<Mutex<RenderBridge>>, prepared: PreparedConstruction) {
    let PreparedConstruction {
        request,
        collision_shape,
        native_shape_width,
        native_shape_height,
        native_shape_radius,
        dynamic_body,
        mass,
        sprite_bound,
        sprite_region,
        composite_sprite,
    } = prepared;
    // Only the box and non-physics constructors compare the name with the
    // literal "ground" and bypass GameLua+0x310 on equality. The line,
    // polygon and circle constructors have no corresponding branch.
    let skips_initial_render_index = request.name == "ground"
        && matches!(
            request.kind,
            ConstructorKind::Box | ConstructorKind::NonPhysics
        );
    let active = !request.controllable;
    let fixture_count = collision_shape.fixture_count();
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let physics_creation_order = bridge.allocate_physics_creation_order();
    let body_allocation_slot = request
        .kind
        .has_body()
        .then(|| bridge.allocate_body_allocation_slot());
    let mut scene_object = SceneObject {
        physics_creation_order,
        body_allocation_slot,
        fixture_proxy_ids: vec![None; fixture_count],
        sprite: request.sprite,
        sprite_bound,
        sprite_region,
        composite_sprite,
        texture: None,
        texture_binding: None,
        texture_scale: 1.0,
        x: request.x,
        y: request.y,
        sweep_center_x: request.x as f32,
        sweep_center_y: request.y as f32,
        collision_shape,
        native_shape_width,
        native_shape_height,
        native_shape_radius,
        dynamic_body,
        kinematic_body: false,
        body_mass: 0.0,
        inverse_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
        fixture_mass_data: (0.0, (0.0, 0.0), 0.0),
        moment_of_inertia: None,
        density: request.density,
        fixture_densities: vec![request.density; fixture_count],
        z_order: request.z_order,
        scale_x: 1.0,
        scale_y: 1.0,
        base_scale_x: 1.0,
        base_scale_y: 1.0,
        physics_scale_x: 1.0,
        physics_scale_y: 1.0,
        angle: 0.0,
        sprite_rotation: 0.0,
        pivot_offset_x: 0.0,
        pivot_offset_y: 0.0,
        alpha: 1.0,
        visible: true,
        flash_animation: false,
        velocity_x: 0.0,
        velocity_y: 0.0,
        angular_velocity: 0.0,
        force_x: 0.0,
        force_y: 0.0,
        torque: 0.0,
        restitution: request.restitution,
        friction: request.friction,
        fixture_restitutions: vec![request.restitution; fixture_count],
        fixture_frictions: vec![request.friction; fixture_count],
        linear_damping: 0.0,
        angular_damping: 0.0,
        gravity_scale: 1.0,
        // All native RenderObjectData constructors use -1 for both fields
        // (+0x124/+0x104).
        gravity_category: -1,
        sensor_gravity_mask: -1,
        fixed_rotation: false,
        bullet: false,
        sensor: false,
        active,
        sleeping: false,
        collision_enabled: request.collision_enabled,
        // Every constructor initializes +0x13F to one. Scripts opt bodies
        // into the material-list filter through the native setter.
        block_collision_enabled: true,
        controllable: request.controllable,
        ignores_score: false,
        keep_orientation: false,
        record_velocity: false,
        revert_gravity: false,
        revert_gravity_with_multiplier: false,
        revert_gravity_force: 0.0,
        revert_gravity_max_velocity: 0.0,
        time_since_collision: -1.0,
        level_goal: false,
        bounce_amplitude_multiplier: 0.0,
        bounce_frequency_multiplier: 0.0,
        bounce_active: false,
        bounce_elapsed: 0.0,
        bounce_current_amplitude: 0.0,
        bounce_initial_amplitude: 0.0,
        graphics_flip_enabled: false,
        not_collided: false,
        ignore_motion: false,
        disable_immovable_collisions: false,
        sensor_definition: false,
        horizontal_flip: false,
        sensor_active: false,
        sensor_type: -1,
        sensor_shape_type: 0,
        sensor_radius: -1.0,
        sensor_width: -1.0,
        sensor_height: 0.0,
        sensor_force_angle: 0.0,
        aiming_aid_collideable: false,
        bubble: false,
        collision_group: 0,
        sensor_minimum_force: 0.0,
        sensor_maximum_force: 0.0,
        native_material: 0,
        material: String::new(),
        collision_materials: Vec::new(),
        water_density: 0.0,
        is_water: false,
        decoration: None,
        ray: None,
        dirt: None,
        dirt_holes: Vec::new(),
        // b2BodyDef uses allowSleep=true, awake=true and active=true in
        // sub_100034740/sub_100034FB0. Controllable bodies are deactivated
        // afterwards but retain that awake state.
        motion_started: dynamic_body,
        sleep_time: 0.0,
        native_was_awake: false,
    };
    // CreateFixture immediately calls ResetMassData, installing the exact
    // float32 aggregate mass and moving c to the fixture center of mass.
    scene_object.reset_native_mass_data((request.x, request.y));
    let name = request.name;
    let z_bucket = crate::native_fcvtzs_f32(scene_object.z_order as f32);
    let sheet = crate::native_scene_sheet_id(&scene_object);
    if !skips_initial_render_index {
        // sub_100070278 returns the existing name-map slot and constructors
        // overwrite its RenderObjectData* without removing an older leaf
        // name. A duplicate therefore remains at every prior render position.
        bridge
            .scene_render_index
            .append(z_bucket, sheet, name.clone());
    }
    bridge.scene.insert(name.clone(), scene_object);
    bridge.install_object_broad_phase_proxies(&name, false);
}
