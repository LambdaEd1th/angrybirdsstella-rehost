//! Native joint-definition switch and `b2World::CreateJoint` facade.

mod anchors;
mod insertion;
mod model;
mod parameters;

use mlua::Result as LuaResult;

use crate::RenderBridge;

pub(crate) fn insert_physics_joint(
    bridge: &mut RenderBridge,
    table: &mlua::Table,
) -> LuaResult<()> {
    let Some(geometry) = anchors::decode_joint_geometry(bridge, table)? else {
        return Ok(());
    };
    let parameters = parameters::decode_joint_parameters(table, &geometry)?;
    insertion::insert_joint(bridge, geometry, parameters);
    Ok(())
}
