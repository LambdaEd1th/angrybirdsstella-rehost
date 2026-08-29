//! Independent RenderObjectData mutation members published across GameLua.

use crate::*;

pub(super) fn install_collision_time(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_setTimeSinceCollision",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "native_setTimeSinceCollision")?;
            let value =
                f64::from(native_required_number(&args, 1, "native_setTimeSinceCollision")? as f32);
            if let Some(object) = render
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
            {
                // sub_10005959C writes only RenderObjectData+0x128. The Lua
                // world record remains untouched until normal game logic or
                // frame-state publication changes it independently.
                object.time_since_collision = value;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_revert_gravity(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setRevertGravityWithMultiplier",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setRevertGravityWithMultiplier")?;
            let enabled = native_required_boolean(&args, 1, "setRevertGravityWithMultiplier")?;
            let force = f64::from(
                -(native_required_number(&args, 2, "setRevertGravityWithMultiplier")? as f32),
            );
            let max_velocity =
                f64::from(
                    native_required_number(&args, 3, "setRevertGravityWithMultiplier")? as f32,
                );
            if let Some(object) = render
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
            {
                // sub_10005962C owns +0x12F independently of the plain
                // setRevertGravity byte at +0x12E and creates no Lua fields.
                object.revert_gravity_with_multiplier = enabled;
                object.revert_gravity_force = force;
                object.revert_gravity_max_velocity = max_velocity;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_sensor_range(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setSensorMinimumAndMaximumForces",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setSensorMinimumAndMaximumForces")?;
            let minimum =
                f64::from(
                    native_required_number(&args, 1, "setSensorMinimumAndMaximumForces")? as f32,
                );
            let maximum =
                f64::from(
                    native_required_number(&args, 2, "setSensorMinimumAndMaximumForces")? as f32,
                );
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let object = bridge
                .game_lua_object_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_100031388 writes the generated adapter's float32 values at
            // RenderObjectData+0x10C/+0x110.
            object.sensor_minimum_force = minimum;
            object.sensor_maximum_force = maximum;
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_sprite_rotation(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setSpriteRotation",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setSpriteRotation")?;
            let angle = native_required_number(&args, 1, "setSpriteRotation")?;
            // sub_10003FC88 receives the generated adapter's float32 value,
            // then runs fmodf and conditionally adds the float32 two-pi value.
            let angle = angle as f32;
            let turn = std::f32::consts::PI + std::f32::consts::PI;
            let mut normalized = angle % turn;
            if normalized < 0.0 {
                normalized += turn;
            }
            let normalized = f64::from(normalized);
            if let Some(object) = render
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
            {
                object.sprite_rotation = normalized;
            }
            if let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(name.as_str())? {
                entry.set("spriteAngle", normalized)?;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_velocity_multiplier(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "multiplyVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "multiplyVelocity")?;
            let multiplier = native_required_number(&args, 1, "multiplyVelocity")?;
            if let Some(object) = render
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
                && (object.dynamic_body || object.kinematic_body)
            {
                let multiplier = multiplier as f32;
                object.velocity_x = f64::from((object.velocity_x as f32) * multiplier);
                object.velocity_y = f64::from((object.velocity_y as f32) * multiplier);
                if object.velocity_x != 0.0 || object.velocity_y != 0.0 {
                    object.wake();
                    object.motion_started = true;
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
