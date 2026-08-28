//! PhysicsWorld overlap and ray-cast Lua query bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // `sub_10005411C` reads a physics-space AABB from the supplied table and
    // always pushes a Lua array containing the names of overlapping bodies.
    // Returning no values (the former generic stub behavior) turns that array
    // into nil and crashes the original IslandMap update at `ipairs`.
    let intersection_bridge = Arc::clone(&render);
    globals.set(
        "getIntersectingObjects",
        lua.create_function(move |lua, args: MultiValue| {
            // Direct member sub_10005411C constructs a table wrapper from
            // stack slot -1, then strictly reads all six numeric fields.
            let bounds = native_top_table(&args, "getIntersectingObjects")?;
            let number = |name: &str| -> LuaResult<f32> {
                Ok(table_required_number(&bounds, name, "getIntersectingObjects")? as f32)
            };
            let x = number("x")?;
            let y = number("y")?;
            let left = x + number("left")?;
            let right = x + number("right")?;
            let down = y + number("down")?;
            let up = y + number("up")?;
            let result = lua.create_table()?;
            let bridge = intersection_bridge
                .lock()
                .expect("render bridge lock poisoned");
            // World::QueryAABB is a broad-phase query: it tests the fixture's
            // fat proxy AABB and lets sub_1000934F4 insert the owning b2Body*
            // into a std::set. That set removes duplicate polygon/edge
            // fixtures before the GameLua object-name lookup.
            let query = (left, down, right, up);
            let mut bodies = BTreeMap::new();
            for proxy_id in bridge.dynamic_tree.query(query) {
                let Some((name, _fixture)) = bridge.dynamic_tree.proxy_user_data(proxy_id) else {
                    continue;
                };
                if let Some(object) = bridge.scene.get(name) {
                    // Purple's std::set is ordered by the actual b2Body
                    // pointer, independently of the intrusive world-list
                    // order. The 0xC0 block class allocates ascending slots
                    // and reuses its free-list head first.
                    if let Some(slot) = object.body_allocation_slot {
                        bodies.entry(slot).or_insert_with(|| name.clone());
                    }
                }
            }
            let mut index = 1;
            for name in bodies.values() {
                result.raw_set(index, name.as_str())?;
                index += 1;
            }
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!(
                    "native getIntersectingObjects(x={x}, y={y}, left={}, right={}, down={}, up={}) -> {} objects",
                    left - x,
                    right - x,
                    down - y,
                    up - y,
                    index - 1
                );
            }
            Ok(result)
        })?,
    )?;

    // `sub_10005464C` returns a flat six-value record for every Box2D ray hit:
    // object name, hit x/y, normal x/y, and segment fraction. Keeping that
    // layout is important because the original Lua iterates it in strides of 6.
    let ray_cast_bridge = Arc::clone(&render);
    globals.set(
        "getRayCastedObjects",
        lua.create_function(move |lua, args: MultiValue| {
            let query = native_top_table(&args, "getRayCastedObjects")?;
            let number = |name: &str| -> LuaResult<f64> {
                Ok(f64::from(
                    table_required_number(&query, name, "getRayCastedObjects")? as f32,
                ))
            };
            let start = (number("x1")?, number("y1")?);
            let end = (number("x2")?, number("y2")?);
            let result = lua.create_table()?;
            if ![start.0, start.1, end.0, end.1]
                .into_iter()
                .all(f64::is_finite)
            {
                return Ok(result);
            }
            let input = NativeRayCastInput::complete(start, end);
            let bridge = ray_cast_bridge.lock().expect("render bridge lock poisoned");
            let hits = bridge
                .dynamic_tree
                .ray_cast_candidates(input.start, input.end, input.max_fraction)
                .into_iter()
                .filter_map(|proxy_id| bridge.dynamic_tree.proxy_user_data(proxy_id))
                // sub_10009366C ignores sensor fixtures, but deliberately
                // does not inspect Box2D filter data or Lua's
                // `collisionEnabled`; the Lua `raycast` helper filters the
                // latter after receiving the complete native hit list.
                .filter_map(|(name, fixture)| {
                    let object = bridge.scene.get(name)?;
                    (!object.sensor).then(|| object.ray_cast_fixture_hit(name, input, *fixture))?
                })
                .collect::<Vec<_>>();
            let mut index = 1;
            for hit in &hits {
                result.raw_set(index, hit.name.as_str())?;
                result.raw_set(index + 1, f64::from(hit.point_x as f32))?;
                result.raw_set(index + 2, f64::from(hit.point_y as f32))?;
                result.raw_set(index + 3, f64::from(hit.normal_x as f32))?;
                result.raw_set(index + 4, f64::from(hit.normal_y as f32))?;
                result.raw_set(index + 5, f64::from(hit.fraction as f32))?;
                index += 6;
            }
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!(
                    "native getRayCastedObjects(x1={}, y1={}, x2={}, y2={}) -> {} hits",
                    start.0,
                    start.1,
                    end.0,
                    end.1,
                    hits.len()
                );
            }
            Ok(result)
        })?,
    )?;

    Ok(())
}

fn native_top_table(args: &MultiValue, function: &str) -> LuaResult<Table> {
    match args.iter().next_back() {
        Some(Value::Table(table)) => Ok(table.clone()),
        _ => Err(runtime_error(format!(
            "bad argument to '{function}' (table expected at stack top)"
        ))),
    }
}
