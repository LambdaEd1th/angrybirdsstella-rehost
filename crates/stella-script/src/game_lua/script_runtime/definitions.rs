//! Definition-pack loading and `blockTable.blocks` indexing.

use std::{path::PathBuf, sync::Arc};

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Value};

use super::{
    chunks::execute_script_in,
    environment::{game_environment, install_table_fallback},
};
use crate::{
    NativeLuaObject, describe_value, native_lua_object, retain_native_lua_object, value_string,
};

pub(crate) fn make_script_loader(lua: &Lua, root: Arc<PathBuf>) -> LuaResult<Function> {
    lua.create_function(move |lua, args: MultiValue| {
        if std::env::var_os("STELLA_TRACE_LUA_LOADS").is_some() {
            let rendered = args
                .iter()
                .map(describe_value)
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("loadLuaFile({rendered})");
        }
        let path = args.iter().next().and_then(value_string);
        let child_name = args.iter().nth(1).and_then(value_string);
        let merge = matches!(args.iter().nth(2), Some(Value::Boolean(true)));
        let deep_merge = matches!(args.iter().nth(3), Some(Value::Boolean(true)));
        if std::env::var_os("STELLA_TRACE_LUA_LOADS").is_some() {
            eprintln!("loadLuaFile flags merge={merge} deep={deep_merge}");
        }
        if let Some(path) = path {
            let parent = game_environment(lua)?;
            if let Some(name) = child_name.as_deref().filter(|name| !name.is_empty()) {
                if merge {
                    // Purple sub_100058960 routes the third-argument form to
                    // its persistent blockTable object, not to gamelua[name].
                    // The ordinary form below is the one that publishes a
                    // child environment directly on gamelua.
                    let block_table = match native_lua_object(lua, NativeLuaObject::BlockTable)? {
                        Some(table) => table,
                        None => {
                            let table = lua.create_table()?;
                            parent.raw_set("blockTable", table.clone())?;
                            retain_native_lua_object(
                                lua,
                                NativeLuaObject::BlockTable,
                                Some(&table),
                            )?;
                            table
                        }
                    };
                    install_table_fallback(lua, &block_table, parent.clone())?;
                    let incoming = lua.create_table()?;
                    install_table_fallback(lua, &incoming, parent.clone())?;
                    if deep_merge
                        && let Value::Function(inherits_block) =
                            parent.get::<Value>("inheritsBlock")?
                    {
                        // sub_100058960 copies the inheritance helper into
                        // the isolated pack and defines this local sentinel.
                        // Several pig/randomization definitions pass it as
                        // the second argument to drop parent components.
                        incoming.raw_set("inheritsBlock", inherits_block)?;
                        incoming.raw_set("IGNORE_COMPONENTS", true)?;
                    }
                    execute_script_in(lua, &root, &path, incoming.clone())?;
                    if deep_merge {
                        // sub_100062028 annotates and indexes the definition
                        // tables, then discards their temporary group arrays.
                        index_definition_lists(lua, &block_table, &incoming)?;
                    } else {
                        // sub_10007F33C is a direct table assignment. Reloads
                        // therefore replace the previous pack wholesale.
                        block_table.raw_set(name, incoming)?;
                    }
                } else {
                    let child = lua.create_table()?;
                    install_table_fallback(lua, &child, parent.clone())?;
                    execute_script_in(lua, &root, &path, child.clone())?;
                    parent.set(name, child)?;
                }
            } else {
                execute_script_in(lua, &root, &path, parent)?;
            }
        }
        Ok(())
    })
}

pub(crate) fn index_definition_lists(
    lua: &Lua,
    block_table: &mlua::Table,
    definitions: &mlua::Table,
) -> LuaResult<()> {
    let index = match block_table.raw_get::<Value>("blocks")? {
        Value::Table(table) => table,
        _ => lua.create_table()?,
    };
    let lists = definitions
        .clone()
        .pairs::<Value, Value>()
        .collect::<LuaResult<Vec<_>>>()?;
    for (group_key, value) in lists {
        let Value::String(group) = group_key else {
            continue;
        };
        let Value::Table(list) = value else {
            continue;
        };
        for pair in list.pairs::<Value, Value>() {
            let (key, value) = pair?;
            let definition_index = match key {
                Value::Integer(value) => f64::from(value as f32),
                Value::Number(value) => f64::from(value as f32),
                _ => continue,
            };
            let Value::Table(definition) = value else {
                continue;
            };
            // Purple sub_100062028 annotates every array definition before
            // publishing it through blocks[definition]. Both fields are
            // observable on the same table object returned by blocks.
            definition.raw_set("index", definition_index)?;
            definition.raw_set("group", group.clone())?;
            if let Value::String(name) = definition.raw_get::<Value>("definition")? {
                index.raw_set(name, definition)?;
            }
        }
    }
    block_table.raw_set("blocks", index)?;
    Ok(())
}
