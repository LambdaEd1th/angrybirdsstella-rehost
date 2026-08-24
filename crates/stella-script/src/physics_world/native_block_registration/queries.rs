//! Bound Dirt fixture-point query and render synchronization members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    extension: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
    object_name: String,
) -> LuaResult<()> {
    let joint_bridge = Arc::clone(&render);
    let joint_name = object_name.clone();
    extension.set(
        "isJointAttached",
        lua.create_function(move |_, args: MultiValue| {
            let coordinate_x = value_number_at(&args, 0).ok_or_else(|| {
                runtime_error("DirtMechanics.isJointAttached argument 1 must be number")
            })?;
            let coordinate_y = value_number_at(&args, 1).ok_or_else(|| {
                runtime_error("DirtMechanics.isJointAttached argument 2 must be number")
            })?;
            let bridge = joint_bridge.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.scene.get(&joint_name) else {
                return Ok(false);
            };
            // sub_100020858 uses two float32 fadd instructions before TestPoint;
            // despite the Lua name it does not inspect a joint-anchor list.
            let point = (
                f64::from((object.x as f32) + coordinate_x as f32),
                f64::from((object.y as f32) + coordinate_y as f32),
            );
            Ok(object.collision_contains_world_point(point))
        })?,
    )?;

    // sub_1000208D4 draws background then all current foreground polygons.
    extension.set(
        "render",
        lua.create_function(move |lua, _: MultiValue| {
            ensure_dirt_component(lua, &render, &resources, &data_root, &object_name)?;
            Ok(())
        })?,
    )
}
