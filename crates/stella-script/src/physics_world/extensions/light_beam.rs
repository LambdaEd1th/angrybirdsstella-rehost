//! LightBeam construction (`sub_10005AD4C`) and `plotPath` (`sub_10008B194`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let light_beam_bridge = Arc::clone(render);
    globals.set(
        "makeLightBeam",
        lua.create_function(move |lua, _: MultiValue| {
            let object = lua.create_table()?;
            let disposed = Rc::new(Cell::new(false));
            let plot_disposed = Rc::clone(&disposed);
            let plot_bridge = Arc::clone(&light_beam_bridge);
            object.set(
                "plotPath",
                lua.create_function(move |lua, args: MultiValue| {
                    if plot_disposed.get() {
                        return Ok(false);
                    }
                    // The generated LightBeam method adapter reads one Lua
                    // table after the colon-call receiver. It does not scan
                    // later arguments for a table with a convenient field.
                    let query = native_required_table(&args, 1, "LightBeam.plotPath")?;
                    plot_path(lua, &plot_bridge, &query)
                })?,
            )?;
            object.set(
                "dispose",
                lua.create_function(move |_, _: MultiValue| {
                    disposed.set(true);
                    Ok(())
                })?,
            )?;
            Ok(object)
        })?,
    )
}

fn plot_path(lua: &Lua, render: &Arc<Mutex<RenderBridge>>, query: &mlua::Table) -> LuaResult<bool> {
    let start_point = match query.get::<Value>("startPoint")? {
        Value::Table(point) => point,
        _ => return Err(runtime_error("LightBeam.plotPath startPoint must be table")),
    };
    let angle = table_required_number(query, "startAngle", "LightBeam.plotPath")? as f32;
    let mut current = (
        table_required_number(&start_point, "x", "LightBeam.plotPath")? as f32,
        table_required_number(&start_point, "y", "LightBeam.plotPath")? as f32,
    );
    let (sine, cosine) = angle.sin_cos();
    let direction = (cosine * 10.0_f32, sine * 10.0_f32);
    let mut path = vec![current];
    let mut target = None;

    // The native loop advances one float32 b2Vec2 by ten units until its
    // RayCast callback records a fixture or the integer level bounds are left.
    loop {
        let end = (current.0 + direction.0, current.1 + direction.1);
        let (closest, limits) = {
            let bridge = render.lock().expect("render bridge lock poisoned");
            let closest = bridge
                .scene
                .iter()
                // LightBeam's ReportFixture callback at sub_10008B7B0
                // rejects sensors and b2Shape::e_chain (type 3). It does not
                // inspect the game-side collisionEnabled filter.
                .filter(|(_, object)| {
                    object.active
                        && !object.sensor
                        && !matches!(object.collision_shape, CollisionShape::Line { .. })
                })
                .flat_map(|(name, object)| {
                    object.ray_cast_hits(
                        name,
                        (f64::from(current.0), f64::from(current.1)),
                        (f64::from(end.0), f64::from(end.1)),
                    )
                })
                .min_by(|left, right| left.fraction.total_cmp(&right.fraction));
            (closest, bridge.level_limits)
        };
        if let Some(hit) = closest {
            current = (hit.point_x as f32, hit.point_y as f32);
            target = Some(hit.name);
            path.push(current);
            break;
        }

        current = end;
        path.push(current);
        if f64::from(current.0) < limits[0]
            || f64::from(current.0) > limits[1]
            || f64::from(current.1) < limits[2]
            || f64::from(current.1) > limits[3]
        {
            break;
        }
    }

    let path_table = lua.create_table()?;
    for (index, (x, y)) in path.into_iter().enumerate() {
        let point = lua.create_table()?;
        point.set("x", f64::from(x))?;
        point.set("y", f64::from(y))?;
        path_table.raw_set(index + 1, point)?;
    }
    query.set("path", path_table)?;

    let previous = query.get::<Value>("target")?;
    match target {
        Some(name) => {
            let world = object_world(lua)?;
            let next = match world.raw_get::<Value>(name.as_str())? {
                Value::Table(entry) => entry,
                _ => return Err(runtime_error(format!("Missing object table: {name}"))),
            };
            let changed = match previous {
                Value::Nil => true,
                Value::Table(previous) => !previous.equals(&next)?,
                _ => {
                    return Err(runtime_error(
                        "Tried to get a Lua table from field 'target'".to_owned(),
                    ));
                }
            };
            query.set("target", next)?;
            Ok(changed)
        }
        None => {
            let changed = !matches!(previous, Value::Nil);
            query.set("target", Value::Nil)?;
            Ok(changed)
        }
    }
}
