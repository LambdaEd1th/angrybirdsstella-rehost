//! Native joint-definition switch and `b2World::CreateJoint` facade.

mod anchors;
mod insertion;
mod model;
mod parameters;

use mlua::Result as LuaResult;

use crate::{PhysicsJoint, RenderBridge, runtime_error};

pub(crate) struct CreatedPhysicsJoint {
    pub(crate) joint: PhysicsJoint,
    pub(crate) descriptor_anchors: (f64, f64, f64, f64),
}

pub(crate) fn insert_physics_joint(
    lua: &mlua::Lua,
    bridge: &mut RenderBridge,
    table: &mlua::Table,
    joint_type: f32,
) -> LuaResult<Option<CreatedPhysicsJoint>> {
    let Some(geometry) = anchors::decode_joint_geometry(lua, bridge, table, joint_type)? else {
        return Ok(None);
    };
    let parameters = parameters::decode_joint_parameters(table, &geometry)?;
    let descriptor_anchors = if matches!(geometry.joint_type, 1 | 6) && geometry.coord_type == 0 {
        let first = &bridge.scene[&geometry.first];
        let second = &bridge.scene[&geometry.second];
        (first.x, first.y, second.x, second.y)
    } else {
        (
            native_descriptor_number(table, "x1")?,
            native_descriptor_number(table, "y1")?,
            native_descriptor_number(table, "x2")?,
            native_descriptor_number(table, "y2")?,
        )
    };
    let native_joint_present = geometry.is_physical && !bridge.physics_world_locked;
    if geometry.joint_type == 1 && geometry.is_physical && !native_joint_present {
        // b2World::CreateJoint (sub_10086E470) returns nullptr while locked.
        // Only the distance branch immediately reads b2DistanceJoint+0xA4
        // to publish `length`, so Purple terminates before jointData/Lua
        // publication. Keep that failed boundary but contain the native null
        // dereference as a catchable Lua error.
        return Err(runtime_error(
            "createJoint cannot read a distance joint while the physics world is locked",
        ));
    }
    let name = geometry.name.clone();
    insertion::insert_joint(bridge, geometry, parameters, native_joint_present);
    Ok(Some(CreatedPhysicsJoint {
        joint: bridge.joints[&name].clone(),
        descriptor_anchors,
    }))
}

fn native_descriptor_number(table: &mlua::Table, field: &str) -> LuaResult<f64> {
    Ok(f64::from(table.get::<f32>(field).unwrap_or(0.0)))
}
