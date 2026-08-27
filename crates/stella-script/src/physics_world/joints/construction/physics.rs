//! Native joint-definition switch and `b2World::CreateJoint` facade.

mod anchors;
mod insertion;
mod model;
mod parameters;

use mlua::Result as LuaResult;

use crate::{PhysicsJoint, RenderBridge};

pub(crate) struct CreatedPhysicsJoint {
    pub(crate) joint: PhysicsJoint,
    pub(crate) descriptor_anchors: (f64, f64, f64, f64),
}

pub(crate) fn insert_physics_joint(
    bridge: &mut RenderBridge,
    table: &mlua::Table,
) -> LuaResult<Option<CreatedPhysicsJoint>> {
    let Some(geometry) = anchors::decode_joint_geometry(bridge, table)? else {
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
    let name = geometry.name.clone();
    insertion::insert_joint(bridge, geometry, parameters);
    Ok(Some(CreatedPhysicsJoint {
        joint: bridge.joints[&name].clone(),
        descriptor_anchors,
    }))
}

fn native_descriptor_number(table: &mlua::Table, field: &str) -> LuaResult<f64> {
    Ok(f64::from(table.get::<f32>(field).unwrap_or(0.0)))
}
