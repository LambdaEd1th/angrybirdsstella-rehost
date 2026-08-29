//! `native_resizeRadius` fixture-replacement member (`sub_100059488`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let resize_radius_bridge = Arc::clone(&render);
    globals.set(
        "native_resizeRadius",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "native_resizeRadius")?;
            let radius = f64::from(native_required_number(&args, 1, "native_resizeRadius")? as f32);
            let density =
                f64::from(native_required_number(&args, 2, "native_resizeRadius")? as f32);
            let friction =
                f64::from(native_required_number(&args, 3, "native_resizeRadius")? as f32);
            let restitution =
                f64::from(native_required_number(&args, 4, "native_resizeRadius")? as f32);
            let (old_center, world_locked) = {
                let mut bridge = resize_radius_bridge
                    .lock()
                    .expect("render bridge lock poisoned");
                let old_center = {
                    let object = bridge
                        .game_lua_object_mut(&name)
                        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                    let center = object.world_center();
                    // sub_100059488 writes RenderObjectData::radius before
                    // DestroyFixture. The old fixture remains geometrically
                    // unchanged until its synchronous EndContact callbacks.
                    object.native_shape_radius = radius;
                    center
                };
                (old_center, bridge.physics_world_locked)
            };
            // b2Body::DestroyFixture and CreateFixture both return immediately
            // while b2World::Step owns e_locked.  RenderObjectData::radius was
            // already written above, but the old fixture, proxy, contacts,
            // coefficients, sensor flag and mass data all survive unchanged.
            if world_locked {
                return Ok(());
            }
            let exits = {
                let mut bridge = resize_radius_bridge
                    .lock()
                    .expect("render bridge lock poisoned");
                bridge.remove_object_broad_phase_proxy_state(&name);
                bridge.drain_contacts_for_invalidated_objects(std::slice::from_ref(&name), false)
            };
            dispatch_native_contact_exits(lua, &resize_radius_bridge, &exits)?;
            let mut bridge = resize_radius_bridge
                .lock()
                .expect("render bridge lock poisoned");
            if let Some(object) = bridge.game_lua_object_mut(&name) {
                object.collision_shape = CollisionShape::Circle { radius };
                // This entry installs `a3` directly into b2CircleShape::m_radius;
                // it does not compose with an earlier setPhysicsScale factor.
                object.physics_scale_x = 1.0;
                object.physics_scale_y = 1.0;
                object.fixture_densities = vec![density];
                object.fixture_frictions = vec![friction];
                object.fixture_restitutions = vec![restitution];
                // The replacement fixture definition starts with sensor=false
                // and this helper has no restoration call.
                object.sensor = false;
                object.reset_native_mass_data(old_center);
            }
            bridge.install_object_broad_phase_proxies(&name, false);
            // Unlike native_setDensity and setPhysicsScale, sub_100059488 does
            // not mirror radius or coefficients into objects.world.
            Ok(())
        })?,
    )?;
    Ok(())
}
