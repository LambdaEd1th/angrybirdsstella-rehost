//! Shared `b2Body::DestroyFixture`/`CreateFixture` lifecycle members.

use super::super::arguments::FixtureCoefficients;
use crate::*;

pub(super) fn world_locked(render: &Arc<Mutex<RenderBridge>>) -> bool {
    render
        .lock()
        .expect("render bridge lock poisoned")
        .physics_world_locked
}

pub(super) fn destroy_all(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
) -> LuaResult<()> {
    // b2Body::DestroyFixture reads b2World::e_locked before it touches the
    // intrusive fixture list, contacts, proxies or aggregate mass data.
    if world_locked(render) {
        return Ok(());
    }
    loop {
        let destruction = {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some((fixture, proxy_id, old_center)) =
                bridge.game_lua_object_mut(name).and_then(|object| {
                    let old_center = object.world_center();
                    object
                        .unlink_head_fixture()
                        .map(|(fixture, proxy_id)| (fixture, proxy_id, old_center))
                })
            else {
                break;
            };
            let exits = bridge.drain_contacts_for_destroyed_fixture(name, fixture);
            (fixture, proxy_id, old_center, exits)
        };

        // sub_10086B548 destroys attached contacts synchronously before proxy
        // release. b2Body::DestroyFixture then refreshes aggregate mass data.
        dispatch_native_contact_exits(lua, render, &destruction.3)?;
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        bridge.release_object_fixture_proxy(name, destruction.0, destruction.1);
        if let Some(object) = bridge.game_lua_object_mut(name) {
            object.reset_native_mass_data(destruction.2);
        }
    }
    Ok(())
}

pub(super) fn install_definition(
    object: &mut SceneObject,
    fixture_count: usize,
    coefficients: FixtureCoefficients,
) {
    object.density = coefficients.density;
    object.friction = coefficients.friction;
    object.restitution = coefficients.restitution;
    object.fixture_densities = vec![coefficients.density; fixture_count];
    object.fixture_frictions = vec![coefficients.friction; fixture_count];
    object.fixture_restitutions = vec![coefficients.restitution; fixture_count];
    object.fixture_proxy_ids = vec![None; fixture_count];
    object.sensor = false;
}

pub(super) fn create(
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    fixture: usize,
    old_center: (f64, f64),
    density: f64,
) {
    // b2Body::CreateFixture returns nullptr while the world is locked.  In
    // particular, it does not allocate or head-insert a fixture, install a
    // proxy, refresh mass data or raise the world's new-fixture flag.
    if world_locked(render) {
        return;
    }
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    // sub_10086B454 installs the fixture proxy before head insertion and mass
    // refresh. The vectors already contain the newly inserted fixture here.
    bridge.install_object_fixture_proxy(name, fixture);
    if density > 0.0
        && let Some(object) = bridge.game_lua_object_mut(name)
    {
        object.reset_native_mass_data(old_center);
    }
}

pub(super) fn restore_sensor(render: &Arc<Mutex<RenderBridge>>, name: &str, sensor: bool) {
    if let Some(object) = render
        .lock()
        .expect("render bridge lock poisoned")
        .game_lua_object_mut(name)
    {
        object.sensor = sensor;
        // Replacement definitions start false; SetSensor(true) wakes the body.
        if sensor {
            object.wake();
        }
    }
}
