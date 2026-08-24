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
        lua.create_function(
            move |lua, (group, key, maximum_width): (String, String, f64)| {
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
                let (clipped_lines, widest_line) =
                    native_clip_text_lines(&text, maximum_width as f32, width);
                let result = game_environment(lua)?.get::<mlua::Table>("clippedText")?;
                let lines = lua.create_table()?;
                for (index, line) in clipped_lines.into_iter().enumerate() {
                    lines.raw_set(index + 1, line)?;
                }
                result.set("lines", lines)?;
                result.set("widestLine", widest_line as f64)?;
                Ok(())
            },
        )?,
    )
}
