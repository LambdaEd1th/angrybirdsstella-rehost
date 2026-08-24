//! `drawTexturedRect` adapter/member (`sub_100085230`/`sub_100043D6C`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawTexturedRect",
        lua.create_function(
            move |_,
                  (sprite, left, top, right, bottom, unused): (
                String,
                f64,
                f64,
                f64,
                f64,
                Value,
            )|
                  -> LuaResult<()> {
                let Value::Boolean(_unused) = unused else {
                    return Err(LuaError::RuntimeError(
                        "drawTexturedRect argument 6 must be boolean".to_owned(),
                    ));
                };
                // The generated adapter returns all four numbers in `s0`.
                let [left, top, right, bottom] =
                    [left, top, right, bottom].map(|value| value as f32);
                // sub_100043D6C replaces the complete current GL state with
                // its native default block and leaves it installed. The
                // destination fields are independently converted after the
                // two edge differences are rounded to float32.
                let _destination = [
                    native_fcvtzs_f32(left),
                    native_fcvtzs_f32(top),
                    native_fcvtzs_f32(right - left),
                    native_fcvtzs_f32(bottom - top),
                ];
                let _resource = sprite;
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                bridge.state = RenderState::default();
                // GL_Context slot +152 is `nullsub_298` in Purple's only
                // GLES2 implementation, so this member submits no geometry.
                Ok(())
            },
        )?,
    )
}
