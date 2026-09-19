//! `GameLua::clipText` (`sub_10004F630`) registration and result publication.

use crate::*;
use std::ffi::{CStr, c_char};
use stella_assets::ka3d::BitmapFont;

fn native_required_c_string_bytes(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<Vec<u8>> {
    let Some(Value::String(value)) = values.iter().nth(index) else {
        return Err(runtime_error(format!(
            "bad argument #{} to '{function}' (string expected)",
            index + 1
        )));
    };
    let pointer = value.to_pointer().cast::<c_char>();
    if pointer.is_null() {
        return Err(runtime_error(format!(
            "bad argument #{} to '{function}' (string expected)",
            index + 1
        )));
    }
    // SAFETY: the MultiValue retains the immutable Lua string for this call.
    // sub_1005285CC returns the same C-string view and the generated adapter
    // constructs std::string through strlen, so an embedded NUL terminates it.
    Ok(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec())
}

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
            let group = native_required_c_string_bytes(&args, 0, "clipText")?;
            let key = native_required_c_string_bytes(&args, 1, "clipText")?;
            let maximum_width = native_required_number(&args, 2, "clipText")? as f32;
            let localized = match (std::str::from_utf8(&group), std::str::from_utf8(&key)) {
                (Ok(group), Ok(key)) => {
                    Some(resolve_localized_string(&resources, &locales, group, key)?)
                }
                _ => None,
            };
            let text = localized.unwrap_or_else(|| native_utf8_skipping_invalid(&key));
            // The `0x10004F6B0` empty-input branch publishes an empty result
            // without ever entering the IFont width trampoline.
            let (bitmap_font, system_font) = if text.is_empty() {
                (None, None)
            } else {
                let resources = resources.lock().expect("resource runtime lock poisoned");
                let current = resources.current_font.as_deref().ok_or_else(|| {
                    runtime_error("No font is set while trying to get string width")
                })?;
                (
                    resources
                        .bitmap_font_values
                        .get(current)
                        .cloned()
                        .or_else(|| fonts.get(current).cloned().map(Arc::new)),
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
            // `0x10004FB64..0x10004FB8C` publishes widestLine first, then
            // replaces lines on the constructor-owned result object.
            result.set("widestLine", f64::from(widest_line))?;
            result.set("lines", lines)?;
            Ok(())
        })?,
    )
}
