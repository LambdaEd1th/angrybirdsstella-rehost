//! `removeThemeSprite` native member.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "removeThemeSprite",
        lua.create_function(move |_, args: MultiValue| {
            let name = theme_required_string(&args, 0, "removeThemeSprite")?;
            let layer_index =
                native_theme_layer(theme_required_f32(&args, 1, "removeThemeSprite")?);
            render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_sprites
                .remove(&(layer_index, name));
            Ok(())
        })?,
    )
}
