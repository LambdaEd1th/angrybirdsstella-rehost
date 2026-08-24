//! Sensor-gravity debug drawing member.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let gravity_visual_bridge = Arc::clone(render);
    globals.set(
        "renderGravityVisualsNative",
        lua.create_function(move |_, _: MultiValue| {
            let mut bridge = gravity_visual_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let sensors = bridge
                .scene
                .values()
                .filter(|object| object.sensor && object.visible)
                .filter_map(|object| {
                    object
                        .collision_aabb()
                        .map(|bounds| (object.x, object.y, bounds))
                })
                .collect::<Vec<_>>();
            let clip_rect = bridge.state.clip_rect;
            let mut commands = Vec::new();
            for (x, y, bounds) in sensors {
                push_software_line(
                    &mut commands,
                    (bounds.0, y),
                    (bounds.2, y),
                    1.0,
                    [0.2, 0.8, 1.0, 0.5],
                    clip_rect,
                );
                push_software_line(
                    &mut commands,
                    (x, bounds.1),
                    (x, bounds.3),
                    1.0,
                    [0.2, 0.8, 1.0, 0.5],
                    clip_rect,
                );
            }
            bridge.extend_rect_commands(commands);
            Ok(())
        })?,
    )
}
