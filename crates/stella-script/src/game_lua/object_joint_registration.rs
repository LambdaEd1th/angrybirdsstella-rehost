//! Joint-motor mutation kept separate from RenderObject-owned adapters.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setRevoluteJointSpeed",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setRevoluteJointSpeed")?;
            let speed = native_required_number(&args, 1, "setRevoluteJointSpeed")? as f32;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let pending_destruction = bridge.joint_pending_native_destruction(&name);
            let endpoints =
                if !pending_destruction && let Some(joint) = bridge.joints.get_mut(&name) {
                    // sub_1008696B4 stores float32 and wakes both endpoints even
                    // when the motor speed already has the requested value.
                    joint.motor_speed = Some(f64::from(speed));
                    Some((joint.first.clone(), joint.second.clone()))
                } else {
                    None
                };
            if let Some((first, second)) = endpoints {
                for endpoint in [first, second] {
                    if let Some(object) = bridge.scene.get_mut(&endpoint) {
                        object.motion_started = true;
                        object.wake();
                    }
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
