//! `drawTexturedRect` adapter/member (`sub_100085230`/`sub_100043D6C`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "drawTexturedRect",
        lua.create_function(move |_, args: MultiValue| -> LuaResult<()> {
            // sub_100085230 uses exact STRING, NUMBER and BOOLEAN
            // helpers for slots 1..6; its generated adapter ignores
            // any trailing stack values.
            let sprite = native_required_string(&args, 0, "drawTexturedRect")?;
            let left = native_required_number(&args, 1, "drawTexturedRect")?;
            let top = native_required_number(&args, 2, "drawTexturedRect")?;
            let right = native_required_number(&args, 3, "drawTexturedRect")?;
            let bottom = native_required_number(&args, 4, "drawTexturedRect")?;
            let _unused = native_required_boolean(&args, 5, "drawTexturedRect")?;
            // The generated adapter returns all four numbers in `s0`.
            let [left, top, right, bottom] = [left, top, right, bottom].map(|value| value as f32);
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
        })?,
    )
}
