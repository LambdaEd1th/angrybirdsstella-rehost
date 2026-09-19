//! Theme background/foreground offset members.

use crate::game_lua::theme_world_offsets::set_native_offset_y;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    install_background(lua, globals, Arc::clone(&render))?;
    install_foreground(lua, globals, render)
}

fn install_background(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setThemeOffsetY",
        lua.create_function(move |lua, args: MultiValue| {
            // sub_10003E308 has the strict `(themeName, offset)` ABI.
            let theme_name = theme_required_string(&args, 0, "setThemeOffsetY")?;
            let offset = theme_required_f32(&args, 1, "setThemeOffsetY")?;
            let definitions = required_named_theme_layers(lua, &theme_name, "bgLayers")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_offset_y = f64::from(offset);
            let mut layer_index = 0;
            while layer_index
                < render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .theme_background_layers
                    .len()
            {
                // Native indexes and validates one definition immediately
                // before mutating the corresponding native record. Preserve
                // partial earlier writes if a later definition is malformed.
                let definition = required_named_theme_layer(&definitions, layer_index + 1)?;
                let applied_offset = {
                    let bridge = render.lock().expect("render bridge lock poisoned");
                    if bridge
                        .theme_sprites
                        .iter()
                        .any(|((sprite_layer, _), _)| *sprite_layer == layer_index)
                    {
                        offset
                    } else {
                        // Context virtual +0xE0 (GL_Context::getHeight,
                        // sub_100599418) reports the current drawable height.
                        // Reload scale and sprite ownership for every layer,
                        // using the native per-layer reads.
                        (bridge.game_world_scale as f32 * offset)
                            / (bridge.screen_height as i32 as f32)
                    }
                };
                // sub_10003E308 tests offsetY and then fetches it again. That
                // lookup goes through lua_rawget (sub_1005288AC), not
                // gettable: an __index metamethod must never run here.
                let has_authored_offset =
                    native_lua51_number(&definition.raw_get::<Value>("offsetY")?).is_some();
                let final_offset = if has_authored_offset {
                    let value = definition.raw_get::<Value>("offsetY")?;
                    // sub_10052A014 calls lua_tonumber without a type guard:
                    // if this second read changed type, conversion yields 0.
                    applied_offset + native_lua51_number(&value).unwrap_or(0.0) as f32
                } else {
                    applied_offset
                };
                if let Some(layer) = render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .theme_background_layers
                    .get_mut(layer_index)
                {
                    set_native_offset_y(layer, final_offset);
                }
                layer_index += 1;
            }
            Ok(())
        })?,
    )
}

fn install_foreground(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_setThemeFgLayerOffsetY",
        lua.create_function(move |lua, args: MultiValue| {
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                let rendered = args
                    .iter()
                    .map(describe_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!("native native_setThemeFgLayerOffsetY({rendered})");
            }
            // sub_10003E59C consumes `(themeName, layer, offset)`, applies
            // FCVTZS, addresses the layer one-based and stores one float32.
            let theme_name = theme_required_string(&args, 0, "native_setThemeFgLayerOffsetY")?;
            let index = native_fcvtzs_f32(theme_required_f32(
                &args,
                1,
                "native_setThemeFgLayerOffsetY",
            )?);
            let offset = theme_required_f32(&args, 2, "native_setThemeFgLayerOffsetY")?;
            // sub_10003E59C performs the same throwing themes/name/fgLayers
            // traversal as the background member even though it subsequently
            // writes only the already-created native layer record.
            required_named_theme_layers(lua, &theme_name, "fgLayers")?;
            if index <= 0 {
                return Ok(());
            }
            if let Some(layer) = render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_foreground_layers
                .get_mut(index as usize - 1)
            {
                // Only layer+0x40 changes. Keep the authored top/bottom
                // marker so a later theme refresh can resolve it again.
                set_native_offset_y(layer, offset);
            }
            Ok(())
        })?,
    )
}
