//! `setTheme` (`sub_10004D348`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    libc_random: Arc<Mutex<NativeLibcRandom>>,
) -> LuaResult<()> {
    globals.set(
        "setTheme",
        lua.create_function(move |lua, args: MultiValue| {
            let theme_name = native_required_string(&args, 0, "setTheme")?;
            let environment = game_environment(lua)?;
            let Value::Table(block_table) = environment.get::<Value>("blockTable")? else {
                return Ok(());
            };
            let Value::Table(themes) = block_table.get::<Value>("themes")? else {
                return Ok(());
            };
            let Value::Table(theme) = themes.get::<Value>(theme_name.as_str())? else {
                return Ok(());
            };

            // `sub_10006B9A4` and the per-record animation-timeline parser
            // consume the same process-global CMWC stream as particles.
            // Work on a snapshot and commit it with both completed arrays so
            // a Lua parse error cannot leave the Rust rehost half-installed.
            let mut random = render
                .lock()
                .expect("render bridge lock poisoned")
                .particle_random
                .clone();
            let resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let mut libc_random = libc_random
                .lock()
                .expect("native libc random lock poisoned");
            let background_layers = parse_theme_layers(
                &theme,
                "bgLayers",
                &resources,
                &mut random,
                &mut libc_random,
            )?;
            let foreground_layers = parse_theme_layers(
                &theme,
                "fgLayers",
                &resources,
                &mut random,
                &mut libc_random,
            )?;
            drop(libc_random);
            drop(resources);
            let sky_color = theme_color(&theme, "skyColor")?;
            let ground_color = theme_color(&theme, "groundColor")?.unwrap_or([0.0; 3]);

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.theme_background_layers = background_layers;
            bridge.theme_foreground_layers = foreground_layers;
            bridge.particle_random = random;
            // ThemeSpriteData vectors live inside the native layer records;
            // replacing both arrays destroys their old owned vectors.
            bridge.theme_sprites = NativeThemeSprites::default();
            bridge.theme_offset_y = 0.0;
            if let Some(color) = sky_color {
                bridge.theme_sky_color = color;
            }
            bridge.theme_ground_color = ground_color;
            Ok(())
        })?,
    )
}

fn theme_color(theme: &mlua::Table, field: &str) -> LuaResult<Option<[f32; 3]>> {
    let Value::Table(color) = theme.get::<Value>(field)? else {
        return Ok(None);
    };
    let channel = |name| -> LuaResult<f32> {
        Ok(color
            .get::<Value>(name)
            .ok()
            .as_ref()
            .and_then(native_lua51_number)
            .unwrap_or(0.0) as f32)
    };
    Ok(Some([channel("r")?, channel("g")?, channel("b")?]))
}
