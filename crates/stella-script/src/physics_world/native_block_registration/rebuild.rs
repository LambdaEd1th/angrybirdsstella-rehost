//! Collision draining and Dirt fixture reconstruction (`sub_100020D70`).

use super::PendingDirtCollisions;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    extension: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
    object_name: String,
    pending: PendingDirtCollisions,
) -> LuaResult<()> {
    extension.set(
        "checkCollisions",
        lua.create_function(move |lua, _: MultiValue| {
            let collisions = pending.borrow_mut().drain(..).collect::<Vec<_>>();
            let count = collisions.len() as i64;
            ensure_dirt_component(lua, &render, &resources, &data_root, &object_name)?;
            for collision in collisions {
                process_collision(lua, &render, &object_name, collision)?;
            }
            Ok(count)
        })?,
    )
}

fn process_collision(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    object_name: &str,
    collision: [f64; 5],
) -> LuaResult<()> {
    let hole = {
        let bridge = render.lock().expect("render bridge lock poisoned");
        let Some(object) = bridge.game_lua_object(object_name) else {
            return Ok(());
        };
        // sub_100020D70 performs these subtractions in float32.
        DirtHole {
            local_x: f64::from((collision[0] as f32) - (object.x as f32)),
            local_y: f64::from((collision[1] as f32) - (object.y as f32)),
            radius: collision[4],
        }
    };

    let has_dirt = render
        .lock()
        .expect("render bridge lock poisoned")
        .game_lua_object(object_name)
        .is_some_and(|object| object.dirt.is_some());
    if !has_dirt {
        if let Some(object) = render
            .lock()
            .expect("render bridge lock poisoned")
            .game_lua_object_mut(object_name)
        {
            Arc::make_mut(&mut object.dirt_holes).push(hole);
        }
        return Ok(());
    }

    let world_locked = render
        .lock()
        .expect("render bridge lock poisoned")
        .physics_world_locked;
    if !world_locked {
        destroy_old_fixtures(lua, render, object_name)?;
    }
    let Some((vertices, fixtures, density, friction, restitution)) = ({
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        bridge.game_lua_object_mut(object_name).and_then(|object| {
            Arc::make_mut(&mut object.dirt_holes).push(hole);
            object.dirt.as_mut().map(|dirt| {
                let dirt = Arc::make_mut(dirt);
                dirt.cut(hole);
                (
                    dirt.foreground_paths.first().cloned().unwrap_or_default(),
                    dirt.foreground_fixtures(),
                    dirt.fixture_density,
                    dirt.fixture_friction,
                    dirt.fixture_restitution,
                )
            })
        })
    }) else {
        return Ok(());
    };
    // sub_100020D70 saves each old fixture's m_next before calling
    // DestroyFixture, so the outer traversal still reaches clipping while
    // every locked destruction is a no-op.  The visual Dirt paths are now
    // updated, but CreateFixture also returns null: retain the complete old
    // native collision fixture/proxy/contact/mass representation.
    if world_locked {
        return Ok(());
    }
    let fixture_count = fixtures.len();
    create_replacement_fixtures(
        render,
        object_name,
        fixtures,
        density,
        friction,
        restitution,
    );
    retain_source_vertices(render, object_name, vertices, fixture_count);
    Ok(())
}

fn destroy_old_fixtures(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    object_name: &str,
) -> LuaResult<()> {
    if render
        .lock()
        .expect("render bridge lock poisoned")
        .physics_world_locked
    {
        return Ok(());
    }
    let old_count = render
        .lock()
        .expect("render bridge lock poisoned")
        .game_lua_object(object_name)
        .map_or(0, |object| object.collision_shape.fixture_count());
    // The native member snapshots m_fixtureList and destroys head-first. Each
    // fixture produces synchronous EndContact callbacks before proxy release.
    for _ in 0..old_count {
        let destruction = {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some((fixture, proxy_id)) = bridge
                .game_lua_object_mut(object_name)
                .and_then(SceneObject::unlink_head_fixture)
            else {
                break;
            };
            let exits = bridge.drain_contacts_for_destroyed_fixture(object_name, fixture);
            (fixture, proxy_id, exits)
        };
        dispatch_native_contact_exits(lua, render, &destruction.2)?;
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        bridge.release_object_fixture_proxy(object_name, destruction.0, destruction.1);
        if let Some(object) = bridge.game_lua_object_mut(object_name) {
            let old_center = object.world_center();
            object.reset_native_mass_data(old_center);
        }
    }
    Ok(())
}

fn create_replacement_fixtures(
    render: &Arc<Mutex<RenderBridge>>,
    object_name: &str,
    fixtures: Vec<Vec<(f64, f64)>>,
    density: f64,
    friction: f64,
    restitution: f64,
) {
    if render
        .lock()
        .expect("render bridge lock poisoned")
        .physics_world_locked
    {
        return;
    }
    // CreateFixture installs the proxy before head insertion and resets mass
    // after every positive-density append.
    for fixture_vertices in fixtures {
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        let Some((fixture, old_center)) = bridge.game_lua_object_mut(object_name).map(|object| {
            let old_center = object.world_center();
            let fixture =
                object.append_dirt_fixture(fixture_vertices, density, friction, restitution);
            (fixture, old_center)
        }) else {
            break;
        };
        bridge.install_object_fixture_proxy(object_name, fixture);
        if density > 0.0
            && let Some(object) = bridge.game_lua_object_mut(object_name)
        {
            object.reset_native_mass_data(old_center);
        }
    }
}

fn retain_source_vertices(
    render: &Arc<Mutex<RenderBridge>>,
    object_name: &str,
    vertices: Vec<(f64, f64)>,
    fixture_count: usize,
) {
    if let Some(object) = render
        .lock()
        .expect("render bridge lock poisoned")
        .game_lua_object_mut(object_name)
        && let CollisionShape::Polygon {
            vertices: source_vertices,
            ..
        } = &mut object.collision_shape
    {
        // A contour the ear cutter cannot triangulate must not activate the
        // legacy single-contour fallback when no fixture exists.
        *source_vertices = if fixture_count == 0 {
            Vec::new()
        } else {
            vertices
        };
    }
}
