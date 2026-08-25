//! `GameLua::clipText` (`sub_10004F630`) registration and result publication.

use crate::*;
use stella_assets::ka3d::BitmapFont;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resources: Arc<Mutex<ResourceRuntime>>,
    fonts: Arc<BTreeMap<String, BitmapFont>>,
    locales: Arc<Mutex<LocaleRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "clipText",
        lua.create_function(move |lua, args: MultiValue| {
            // Generated adapter sub_100086070 -> sub_1000860D8 reads two
            // exact strings and one exact number, narrows width to f32,
            // and ignores trailing stack values.
            let group = native_required_string(&args, 0, "clipText")?;
            let key = native_required_string(&args, 1, "clipText")?;
            let maximum_width = native_required_number(&args, 2, "clipText")? as f32;
            let text = {
                let locales = locales.lock().expect("locale runtime lock poisoned");
                locales
                    .loaded
                    .get(&locales.current)
                    .and_then(|locale| locale.get(&group))
                    .and_then(|strings| strings.get(&key))
                    .cloned()
                    .unwrap_or(key)
            };
            let (bitmap_font, system_font) = {
                let resources = resources.lock().expect("resource runtime lock poisoned");
                let current = resources.current_font.as_deref().ok_or_else(|| {
                    runtime_error("No font is set while trying to get string width")
                })?;
                (
                    fonts.get(current).cloned(),
                    resources.system_fonts.get(current).cloned(),
                )
            };
            let width = |line: &str| {
                system_font.as_ref().map_or_else(
                    || {
                        bitmap_font
                            .as_ref()
                            .map_or(0, |font| bitmap_font_string_width(font, line))
                    },
                    |font| system_font_string_width(font, line),
                )
            };
            let (clipped_lines, widest_line) = native_clip_text_lines(&text, maximum_width, width);
            // sub_10004F630 writes GameLua+0x430 directly. Replacing the
            // script-visible `clippedText` field does not retarget this
            // constructor-owned result object.
            let result = native_lua_object(lua, NativeLuaObject::ClippedText)?
                .ok_or_else(|| runtime_error("clippedText is not a table"))?;
            let lines = lua.create_table()?;
            for (index, line) in clipped_lines.into_iter().enumerate() {
                lines.raw_set(index + 1, line)?;
            }
            result.set("lines", lines)?;
            result.set("widestLine", widest_line as f64)?;
            Ok(())
        })?,
    )
}
