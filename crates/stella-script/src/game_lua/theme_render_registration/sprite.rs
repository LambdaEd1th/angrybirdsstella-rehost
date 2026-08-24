//! `setThemeSprite` (`sub_10003E0F8`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setThemeSprite",
        lua.create_function(move |_, args: MultiValue| {
            let old_sprite = theme_required_string(&args, 0, "setThemeSprite")?;
            let new_sprite = theme_required_string(&args, 1, "setThemeSprite")?;
            let index = native_theme_layer(theme_required_f32(&args, 2, "setThemeSprite")?);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            if let Some(sprite) = bridge.theme_sprites.get_mut(&(index, old_sprite)) {
                // This changes ThemeSpriteData::sprite; it does not replace
                // the layer's own repeating background image.
                sprite.sprite = new_sprite;
            }
            Ok(())
        })?,
    )
}
