//! Bound `DirtMechanics::onCollision` adapter (`sub_100020560`).

use super::PendingDirtCollisions;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    extension: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    pending: PendingDirtCollisions,
) -> LuaResult<()> {
    extension.set(
        "onCollision",
        lua.create_function(move |_, args: MultiValue| {
            // Fixed slots: x, y, normalX, normalY, radius, colliderName,
            // afterVelocityX, afterVelocityY. The function is already bound,
            // so an explicit colon-call table is a native type error.
            let required_number = |index: usize| {
                value_number_at(&args, index).ok_or_else(|| {
                    runtime_error(format!(
                        "DirtMechanics.onCollision argument {} must be number",
                        index + 1
                    ))
                })
            };
            let values = [
                required_number(0)?,
                required_number(1)?,
                required_number(2)?,
                required_number(3)?,
                required_number(4)?,
            ];
            let collider = args.iter().nth(5).and_then(value_string).ok_or_else(|| {
                runtime_error("DirtMechanics.onCollision argument 6 must be string")
            })?;
            let after_velocity_x = f64::from(required_number(6)? as f32);
            let after_velocity_y = f64::from(required_number(7)? as f32);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // The native member resolves the live RenderObject before writing
            // into sub_10005E860's delayed-velocity map.
            if bridge.game_lua_object_exists(&collider) {
                bridge
                    .collision_velocities
                    .insert(collider, (after_velocity_x, after_velocity_y));
            }
            drop(bridge);

            // DirtMechanics::Collision is five adjacent float32 values.
            pending.borrow_mut().push([
                f64::from(values[0] as f32),
                f64::from(values[1] as f32),
                f64::from(values[2] as f32),
                f64::from(values[3] as f32),
                f64::from(values[4] as f32),
            ]);
            Ok(())
        })?,
    )
}
