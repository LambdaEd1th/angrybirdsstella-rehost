//! Main `GameLua::setPhysicsScale` member (`sub_10004050C`).

use super::{arguments, fixture_rebuild};
use crate::game_lua::object_scale_member;
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShapeKind {
    None,
    Polygon,
    Circle,
    Unsupported,
}

#[derive(Clone, Copy)]
struct ObjectSnapshot {
    kind: ShapeKind,
    old_scale_x: f32,
    old_scale_y: f32,
    native_width: f32,
    native_height: f32,
    sensor: bool,
}

pub(super) fn apply(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    scale_x: f64,
    scale_y: f64,
) -> LuaResult<()> {
    // sub_10004050C performs both lookups before calling sub_100040304.
    let entry = arguments::required_world_entry(lua, name)?;
    let snapshot = snapshot(render, name)?;
    object_scale_member::apply(lua, render, name, scale_x, scale_y)?;

    match snapshot.kind {
        ShapeKind::None => Ok(()),
        ShapeKind::Unsupported => Err(runtime_error(format!(
            "Attempted to resize object {name} whose shape is of an unsupported type"
        ))),
        ShapeKind::Polygon => {
            resize_polygon_lua_fields(render, &entry, name, scale_x, scale_y, snapshot)?;
            let coefficients = arguments::coefficients(&entry)?;
            let ratios = (
                (scale_x as f32) / snapshot.old_scale_x,
                (scale_y as f32) / snapshot.old_scale_y,
            );
            fixture_rebuild::polygon(lua, render, name, ratios, coefficients, snapshot.sensor)
        }
        ShapeKind::Circle => {
            let definition_scale = arguments::circle_definition_scale(lua, &entry)?;
            let fixture_scale =
                ((scale_x as f32).min(scale_y as f32) / definition_scale).abs() + 0.0001_f32;
            let radius = arguments::required_f32(&entry, "radius")?;
            let coefficients = arguments::coefficients(&entry)?;
            fixture_rebuild::circle(
                lua,
                render,
                name,
                radius,
                fixture_scale,
                coefficients,
                snapshot.sensor,
            )
        }
    }
}

fn snapshot(render: &Arc<Mutex<RenderBridge>>, name: &str) -> LuaResult<ObjectSnapshot> {
    let bridge = render.lock().expect("render bridge lock poisoned");
    let object = bridge
        .game_lua_object(name)
        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
    let kind = match object.collision_shape {
        CollisionShape::None => ShapeKind::None,
        CollisionShape::Box { .. } | CollisionShape::Polygon { .. } => ShapeKind::Polygon,
        CollisionShape::Circle { .. } => ShapeKind::Circle,
        CollisionShape::Line { .. } => ShapeKind::Unsupported,
    };
    Ok(ObjectSnapshot {
        kind,
        old_scale_x: object.scale_x as f32,
        old_scale_y: object.scale_y as f32,
        native_width: object.native_shape_width as f32,
        native_height: object.native_shape_height as f32,
        sensor: object.sensor,
    })
}

fn resize_polygon_lua_fields(
    render: &Arc<Mutex<RenderBridge>>,
    entry: &mlua::Table,
    name: &str,
    scale_x: f64,
    scale_y: f64,
    snapshot: ObjectSnapshot,
) -> LuaResult<()> {
    let ratio_x = (scale_x as f32) / snapshot.old_scale_x;
    let ratio_y = (scale_y as f32) / snapshot.old_scale_y;
    let width = (snapshot.native_width * ratio_x).abs();
    let height = (snapshot.native_height * ratio_y).abs();
    {
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        let object = bridge
            .game_lua_object_mut(name)
            .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
        object.native_shape_width = f64::from(width);
        object.native_shape_height = f64::from(height);
    }
    // These writes precede all three coefficient reads in the native member.
    entry.set("width", f64::from(width))?;
    entry.set("height", f64::from(height))?;
    Ok(())
}
