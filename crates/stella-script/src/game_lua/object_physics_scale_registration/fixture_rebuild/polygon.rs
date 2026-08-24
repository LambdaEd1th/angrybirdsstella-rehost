//! Polygon/box fixture member (`sub_100067CE8`, 712 bytes/42 blocks in IDA).

use super::super::arguments::FixtureCoefficients;
use super::lifecycle;
use crate::*;

enum SavedStorage {
    Box {
        width: f64,
        height: f64,
    },
    Polygon {
        source_vertices: Vec<(f64, f64)>,
        fixture_list_order: Vec<Vec<(f64, f64)>>,
    },
}

pub(super) fn rebuild(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    ratios: (f32, f32),
    coefficients: FixtureCoefficients,
    sensor: bool,
) -> LuaResult<()> {
    let storage = save_fixture_list(render, name)?;
    lifecycle::destroy_all(lua, render, name)?;

    match storage {
        SavedStorage::Box { width, height } => {
            create_box(render, name, width, height, ratios, coefficients);
        }
        SavedStorage::Polygon {
            source_vertices,
            fixture_list_order,
        } => create_polygon(
            render,
            name,
            source_vertices,
            fixture_list_order,
            ratios,
            coefficients,
        ),
    }
    lifecycle::restore_sensor(render, name, sensor);
    Ok(())
}

fn save_fixture_list(render: &Arc<Mutex<RenderBridge>>, name: &str) -> LuaResult<SavedStorage> {
    let bridge = render.lock().expect("render bridge lock poisoned");
    let object = bridge
        .scene
        .get(name)
        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
    match &object.collision_shape {
        CollisionShape::Box { width, height } => Ok(SavedStorage::Box {
            width: *width,
            height: *height,
        }),
        CollisionShape::Polygon { vertices, fixtures } => {
            let mut fixture_list_order = if fixtures.is_empty() {
                vec![vertices.clone()]
            } else {
                fixtures.clone()
            };
            // b2Body::m_fixtureList is head-inserted. The native member walks
            // head-to-tail and recreates in that order, reversing the new list.
            fixture_list_order.reverse();
            Ok(SavedStorage::Polygon {
                source_vertices: vertices.clone(),
                fixture_list_order,
            })
        }
        _ => Err(runtime_error(format!(
            "Attempted to resize object {name} whose shape is of an unsupported type"
        ))),
    }
}

fn create_box(
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    width: f64,
    height: f64,
    ratios: (f32, f32),
    coefficients: FixtureCoefficients,
) {
    let old_center = {
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        bridge.scene.get_mut(name).map(|object| {
            let old_center = object.world_center();
            object.collision_shape = CollisionShape::Box { width, height };
            object.physics_scale_x = f64::from((object.physics_scale_x as f32) * ratios.0);
            object.physics_scale_y = f64::from((object.physics_scale_y as f32) * ratios.1);
            lifecycle::install_definition(object, 1, coefficients);
            old_center
        })
    };
    if let Some(old_center) = old_center {
        lifecycle::create(render, name, 0, old_center, coefficients.density);
    }
}

fn create_polygon(
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    source_vertices: Vec<(f64, f64)>,
    fixture_list_order: Vec<Vec<(f64, f64)>>,
    ratios: (f32, f32),
    coefficients: FixtureCoefficients,
) {
    if let Some(object) = render
        .lock()
        .expect("render bridge lock poisoned")
        .scene
        .get_mut(name)
    {
        object.physics_scale_x = f64::from((object.physics_scale_x as f32) * ratios.0);
        object.physics_scale_y = f64::from((object.physics_scale_y as f32) * ratios.1);
        object.collision_shape = CollisionShape::Polygon {
            vertices: Vec::new(),
            fixtures: Vec::new(),
        };
        lifecycle::install_definition(object, 0, coefficients);
    }

    for vertices in fixture_list_order {
        let next = {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.scene.get_mut(name).map(|object| {
                let old_center = object.world_center();
                let fixture = object.append_dirt_fixture(
                    vertices,
                    coefficients.density,
                    coefficients.friction,
                    coefficients.restitution,
                );
                (fixture, old_center)
            })
        };
        let Some((fixture, old_center)) = next else {
            break;
        };
        lifecycle::create(render, name, fixture, old_center, coefficients.density);
    }

    if let Some(object) = render
        .lock()
        .expect("render bridge lock poisoned")
        .scene
        .get_mut(name)
        && let CollisionShape::Polygon { vertices, .. } = &mut object.collision_shape
    {
        *vertices = source_vertices;
    }
}
