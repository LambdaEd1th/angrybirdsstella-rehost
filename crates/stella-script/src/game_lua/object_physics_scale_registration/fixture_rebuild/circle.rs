//! Circle-fixture branch embedded in `sub_10004050C`.

use super::super::arguments::FixtureCoefficients;
use super::lifecycle;
use crate::*;

pub(super) fn rebuild(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    radius: f64,
    fixture_scale: f32,
    coefficients: FixtureCoefficients,
    sensor: bool,
) -> LuaResult<()> {
    // RenderObjectData::radius changes before DestroyFixture, while the live
    // fixture retains its old circle until synchronous EndContact completes.
    if let Some(object) = render
        .lock()
        .expect("render bridge lock poisoned")
        .game_lua_object_mut(name)
    {
        object.native_shape_radius = f64::from(fixture_scale * radius as f32);
    }
    // sub_10004050C writes RenderObjectData::radius before entering the two
    // b2Body members.  Both members then reject the operation under e_locked,
    // leaving the old fixture geometry, coefficients, proxy and mass intact.
    if lifecycle::world_locked(render) {
        return Ok(());
    }
    lifecycle::destroy_all(lua, render, name)?;

    let old_center = {
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        bridge.game_lua_object_mut(name).map(|object| {
            let old_center = object.world_center();
            object.collision_shape = CollisionShape::Circle { radius };
            object.physics_scale_x = f64::from(fixture_scale);
            object.physics_scale_y = f64::from(fixture_scale);
            lifecycle::install_definition(object, 1, coefficients);
            old_center
        })
    };
    if let Some(old_center) = old_center {
        lifecycle::create(render, name, 0, old_center, coefficients.density);
    }
    lifecycle::restore_sensor(render, name, sensor);
    Ok(())
}
