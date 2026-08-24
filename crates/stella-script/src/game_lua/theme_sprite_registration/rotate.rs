//! `rotateThemeSprites` native member.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "rotateThemeSprites",
        lua.create_function(move |_, args: MultiValue| {
            // sub_10003DF34 uses float32 3.1416f twice rather than the
            // engine's higher-precision PI constant.
            const NATIVE_TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);
            let delta = theme_required_f32(&args, 0, "rotateThemeSprites")?;
            for sprite in render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_sprites
                .values_mut()
            {
                let mut angle = (sprite.angular_velocity as f32)
                    .mul_add(delta, sprite.angle as f32)
                    % NATIVE_TWO_PI;
                if angle < 0.0 {
                    angle += NATIVE_TWO_PI;
                }
                sprite.angle = f64::from(angle);
            }
            Ok(())
        })?,
    )
}
