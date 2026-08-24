//! Zero-argument block-editor definition loader (`sub_100044F90`).

use crate::*;

pub(crate) fn install(lua: &Lua, globals: &mlua::Table, data_root: Arc<PathBuf>) -> LuaResult<()> {
    globals.set(
        "loadBlocksForEditing",
        lua.create_function(move |lua, _: MultiValue| {
            // sub_100044F90 creates `blockEditorTable`, then loads this exact
            // legacy module sequence below `scriptPath`. Retail Stella omits
            // several editor-only packs, so missing optional packs are
            // skipped just as the resource layer does in the shipped build.
            const EDITOR_MODULES: [(&str, &str); 14] = [
                ("blocks_levelgoals.lua", "blocks_levelgoals"),
                ("blocks_scoreobjects.lua", "blocks_scoreobjects"),
                ("birds.lua", "birds"),
                ("blocks_hazard.lua", "blocks_hazard"),
                ("blocks_wood.lua", "blocks_wood"),
                ("blocks_stone.lua", "blocks_stone"),
                ("blocks_glass.lua", "blocks_glass"),
                ("blocks_gameElements.lua", "blocks_gameElements"),
                ("blocks_static.lua", "blocks_static"),
                ("blocks_planets.lua", "blocks_planets"),
                ("blocks_asteroids.lua", "blocks_asteroids"),
                ("blocks_decorations.lua", "blocks_decorations"),
                ("blocks_sensors.lua", "blocks_sensors"),
                ("groups.lua", "groups"),
            ];

            let environment = game_environment(lua)?;
            let editor = lua.create_table()?;
            install_table_fallback(lua, &editor, environment.clone())?;
            environment.set("blockEditorTable", editor.clone())?;
            lua.globals().set("blockEditorTable", editor.clone())?;
            let script_path = environment
                .get::<String>("scriptPath")
                .unwrap_or_else(|_| "scripts/definitions".to_owned());
            for (file_name, module_name) in EDITOR_MODULES {
                let requested = format!("{}/{file_name}", script_path.trim_end_matches('/'));
                let present =
                    resolve_script(&data_root, &requested).is_ok_and(|path| path.is_file());
                if present {
                    load_script_to_object(
                        lua,
                        &data_root,
                        &requested,
                        Some(editor.clone()),
                        Some(module_name),
                    )?;
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
