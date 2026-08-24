//! Theme background/foreground offset members.

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
            const SCREEN_HEIGHT: f32 = 768.0;

            // sub_10003E308 has the strict `(themeName, offset)` ABI.
            let theme_name = theme_required_string(&args, 0, "setThemeOffsetY")?;
            let offset = theme_required_f32(&args, 1, "setThemeOffsetY")?;
            let authored_offsets = named_theme_layer_offsets(lua, &theme_name, "bgLayers")?;

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.theme_offset_y = f64::from(offset);
            let game_world_scale = bridge.game_world_scale as f32;
            let layers_with_sprites = (0..bridge.theme_background_layers.len())
                .map(|layer_index| {
                    bridge
                        .theme_sprites
                        .iter()
                        .any(|((sprite_layer, _), _)| *sprite_layer == layer_index)
                })
                .collect::<Vec<_>>();
            for (layer_index, layer) in bridge.theme_background_layers.iter_mut().enumerate() {
                let applied_offset = if layers_with_sprites[layer_index] {
                    offset
                } else {
                    (game_world_scale * offset) / SCREEN_HEIGHT
                };
                let authored_offset = authored_offsets
                    .get(layer_index)
                    .copied()
                    .flatten()
                    .unwrap_or(0.0);
                layer.offset_y =
                    ThemeVerticalOffset::Pixels(f64::from(applied_offset + authored_offset));
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
        lua.create_function(move |_, args: MultiValue| {
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
            let _theme_name = theme_required_string(&args, 0, "native_setThemeFgLayerOffsetY")?;
            let index = native_fcvtzs_f32(theme_required_f32(
                &args,
                1,
                "native_setThemeFgLayerOffsetY",
            )?);
            let offset = theme_required_f32(&args, 2, "native_setThemeFgLayerOffsetY")?;
            if index <= 0 {
                return Ok(());
            }
            if let Some(layer) = render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_foreground_layers
                .get_mut(index as usize - 1)
            {
                layer.offset_y = ThemeVerticalOffset::Pixels(f64::from(offset));
            }
            Ok(())
        })?,
    )
}
