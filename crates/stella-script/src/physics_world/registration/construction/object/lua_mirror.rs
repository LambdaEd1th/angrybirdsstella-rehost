//! Immediate `objects.world` mirror written by every native constructor.

use mlua::{Lua, Result as LuaResult};

use crate::object_world;

use super::super::{ConstructorKind, PreparedConstruction};

pub(super) fn replace(lua: &Lua, prepared: &PreparedConstruction) -> LuaResult<()> {
    let request = &prepared.request;
    let world = object_world(lua)?;
    // Every constructor calls sub_100529C84 for a fresh table before
    // sub_10007F33C overwrites objects.world[name]. It does not update a
    // previously published table in place when a name is reused.
    let entry = lua.create_table()?;
    entry.set("name", request.name.as_str())?;
    entry.set("sprite", request.sprite.as_str())?;
    match request.kind {
        ConstructorKind::Box => {
            entry.set("type", "box")?;
            entry.set("width", request.shape_width)?;
            entry.set("height", request.shape_height)?;
        }
        ConstructorKind::Circle => {
            entry.set("type", "circle")?;
            entry.set("radius", request.shape_radius)?;
        }
        ConstructorKind::Polygon => {
            entry.set("type", "polygon")?;
            entry.set("width", request.shape_width)?;
            entry.set("height", request.shape_height)?;
        }
        ConstructorKind::Line => {
            entry.set("type", "line")?;
            entry.set("width", request.shape_width)?;
            entry.set("height", request.shape_height)?;
        }
        ConstructorKind::NonPhysics => entry.set("type", "none")?,
    }
    entry.set("x", request.x)?;
    entry.set("y", request.y)?;
    entry.set("angle", 0.0_f64)?;
    entry.set("density", request.density)?;
    entry.set("friction", request.friction)?;
    entry.set("restitution", request.restitution)?;
    entry.set("mass", prepared.mass)?;
    entry.set("xVel", 0.0_f64)?;
    entry.set("yVel", 0.0_f64)?;
    entry.set("z_order", request.z_order)?;
    entry.set("animTimer", 0.0_f64)?;
    entry.set("animFrame", 1.0_f64)?;
    entry.set("animThresholdTimer", 0.0_f64)?;
    entry.set("collisionEnabled", request.collision_enabled)?;
    entry.set("alpha", 1.0_f64)?;
    world.raw_set(request.name.as_str(), entry)
}
