//! Large `setObjectParameter` switch `sub_10004EF74`, registered at `0x10002E4E8`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setObjectParameter",
        lua.create_function(move |lua, args: MultiValue| {
            // sub_1000889E4 validates all three fixed slots before invoking
            // sub_10004EF74. Both numeric slots are narrowed to float32.
            let name = native_required_string(&args, 0, "setObjectParameter")?;
            let parameter =
                native_fcvtzs_f32(native_required_number(&args, 1, "setObjectParameter")? as f32);
            let native_value = native_required_number(&args, 2, "setObjectParameter")? as f32;
            let value = f64::from(native_value);
            let integer_value = native_fcvtzs_f32(native_value);
            let enabled = integer_value == 1;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setObjectParameter({name:?}, {parameter}, {native_value})");
            }

            // Parameter 22 reaches b2Body::SetActive. Its contact callbacks
            // occur synchronously while the body is being deactivated.
            if parameter == 22 {
                let exits = {
                    let mut bridge = render.lock().expect("render bridge lock poisoned");
                    let Some(object) = bridge.game_lua_object_mut(&name) else {
                        return Err(runtime_error(format!("Missing object: {name}")));
                    };
                    if !object.has_physics_body() {
                        return Ok(());
                    }
                    if object.sensor_definition {
                        object.sensor_active = enabled;
                    }
                    let was_active = object.active;
                    bridge.set_object_active_state(&name, enabled);
                    if was_active && !enabled {
                        bridge.drain_contacts_for_invalidated_objects(
                            std::slice::from_ref(&name),
                            false,
                        )
                    } else {
                        Vec::new()
                    }
                };
                return dispatch_native_contact_exits(lua, &render, &exits);
            }

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let physics_world_locked = bridge.physics_world_locked;
            let Some(object) = bridge.game_lua_object_mut(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            let mut body_type_changed = false;
            match parameter {
                1 => {
                    object.level_goal = enabled;
                    if enabled {
                        object.decoration = None;
                    }
                }
                2 if object.has_physics_body() && !physics_world_locked => {
                    body_type_changed =
                        object.set_native_body_type(if native_value == 0.0 { 0 } else { 2 });
                }
                5 => {
                    object.scale_x = value;
                    object.scale_y = value;
                    object.base_scale_x = value;
                    object.base_scale_y = value;
                }
                6 => object.bounce_amplitude_multiplier = value,
                7 => object.bounce_frequency_multiplier = value,
                8 => object.horizontal_flip = enabled,
                9 => object.graphics_flip_enabled = enabled,
                11 => object.not_collided = enabled,
                12 => object.ignore_motion = enabled,
                15 => object.disable_immovable_collisions = enabled,
                16 if object.has_physics_body() => {
                    object.gravity_scale = if enabled { 0.0 } else { 1.0 };
                }
                17 => {
                    object.scale_x = value;
                    object.base_scale_x = value;
                }
                18 => {
                    object.scale_y = value;
                    object.base_scale_y = value;
                }
                20 => object.sensor_definition = enabled,
                21 if object.has_physics_body() => {
                    object.sensor_type = integer_value;
                    if matches!(integer_value, 5 | 7) {
                        let old_center = object.world_center();
                        object.fixed_rotation = true;
                        object.bullet = true;
                        object.reset_native_mass_data(old_center);
                    }
                }
                24 => object.sensor_shape_type = integer_value,
                25 => object.sensor_minimum_force = value,
                26 => object.sensor_maximum_force = value,
                27 => {
                    object.sensor_radius = value;
                    if object.has_physics_body()
                        && let CollisionShape::Circle { radius } = &mut object.collision_shape
                    {
                        *radius = value;
                    }
                }
                28 => object.sensor_width = value,
                29 => object.sensor_force_angle = value,
                31 => object.time_since_collision = value,
                32 => object.aiming_aid_collideable |= enabled,
                33 => object.visible = enabled,
                34 => object.bubble = enabled,
                35 => object.sensor_height = value,
                36 => object.collision_group = integer_value,
                37 if object.has_physics_body() && !physics_world_locked => {
                    body_type_changed =
                        object.set_native_body_type(if native_value == 1.0 { 1 } else { 2 });
                }
                // b2Body::SetMassData tests e_locked before reading or
                // writing the body mass state. The outer parameter switch
                // has no reflected RenderObjectData field for this case.
                38 if object.has_physics_body() && !physics_world_locked => {
                    object.set_native_mass_data_from_origin_inertia(native_value);
                }
                39 if object.has_physics_body()
                    && !physics_world_locked
                    && (0..=2).contains(&integer_value) =>
                {
                    body_type_changed = object.set_native_body_type(integer_value);
                }
                // Cases 3/4/10/13/14/19/23/30 and out-of-range values are
                // explicit no-ops in sub_10004EF74's switch table.
                _ => {}
            }
            if body_type_changed {
                bridge.flag_contacts_for_filtering_for_body(&name);
            }
            Ok(())
        })?,
    )
}
