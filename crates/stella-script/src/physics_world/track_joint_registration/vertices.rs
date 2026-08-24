//! Native object-fixture vertex table binding.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "getObjectVertices",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "getObjectVertices")?;
            let result = lua.create_table()?;
            let bridge = render.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.scene.get(&name) else {
                return Ok(result);
            };
            let fixtures = match &object.collision_shape {
                CollisionShape::Box { width, height } => vec![vec![
                    (-width * 0.5, -height * 0.5),
                    (width * 0.5, -height * 0.5),
                    (width * 0.5, height * 0.5),
                    (-width * 0.5, height * 0.5),
                ]],
                CollisionShape::Polygon { fixtures, .. } => fixtures.clone(),
                CollisionShape::Line { vertices } => vec![vertices.clone()],
                CollisionShape::Circle { .. } | CollisionShape::None => Vec::new(),
            };
            // sub_10005A7BC follows b2Body::m_fixtureList. CreateFixture
            // prepends, whereas Rust retains creation order, so emit in reverse.
            // It adds the body position to already-scaled f32 vertices without
            // applying body rotation.
            for (fixture_index, vertices) in fixtures.into_iter().rev().enumerate() {
                if vertices.is_empty() {
                    continue;
                }
                let fixture = lua.create_table()?;
                for (index, (x, y)) in vertices.into_iter().enumerate() {
                    let point = lua.create_table()?;
                    let native_x = (object.x as f32) + (x as f32) * (object.physics_scale_x as f32);
                    let native_y = (object.y as f32) + (y as f32) * (object.physics_scale_y as f32);
                    point.set("x", native_x)?;
                    point.set("y", native_y)?;
                    fixture.raw_set(index + 1, point)?;
                }
                result.raw_set(fixture_index + 1, fixture)?;
            }
            Ok(result)
        })?,
    )?;
    Ok(())
}
