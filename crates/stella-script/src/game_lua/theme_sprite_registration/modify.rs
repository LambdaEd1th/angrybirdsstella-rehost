//! `modifyThemeSprite` native member.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "modifyThemeSprite",
        lua.create_function(move |_, args: MultiValue| {
            let name = theme_required_string(&args, 0, "modifyThemeSprite")?;
            let x = theme_required_f32(&args, 1, "modifyThemeSprite")?;
            let y = theme_required_f32(&args, 2, "modifyThemeSprite")?;
            let scale_x = theme_required_f32(&args, 3, "modifyThemeSprite")?;
            let scale_y = theme_required_f32(&args, 4, "modifyThemeSprite")?;
            let angle = theme_required_f32(&args, 5, "modifyThemeSprite")?;
            let layer_index =
                native_theme_layer(theme_required_f32(&args, 6, "modifyThemeSprite")?);
            if let Some(sprite) = render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_sprites
                .get_mut(&(layer_index, name))
            {
                sprite.x = f64::from(x);
                sprite.y = f64::from(y);
                sprite.scale_x = f64::from(scale_x);
                sprite.scale_y = f64::from(scale_y);
                sprite.angle = f64::from(angle);
            }
            Ok(())
        })?,
    )
}
