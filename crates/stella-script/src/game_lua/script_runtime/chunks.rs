//! Lua source/bytecode preparation and execution.

use std::{fs, path::Path};

use mlua::{Lua, Result as LuaResult};

use super::paths::{resolve_script, runtime_error};
use crate::decode_persistent_lua;

pub(crate) fn execute_script(lua: &Lua, data_root: &Path, requested: &str) -> LuaResult<()> {
    let path = resolve_script(data_root, requested).map_err(runtime_error)?;
    let bytes = fs::read(&path).map_err(runtime_error)?;
    let bytes = decode_persistent_lua(bytes);
    let prepared = prepare_lua_chunk(&bytes).map_err(runtime_error)?;
    lua.load(&prepared).set_name(path.to_string_lossy()).exec()
}

pub(crate) fn execute_script_in(
    lua: &Lua,
    data_root: &Path,
    requested: &str,
    environment: mlua::Table,
) -> LuaResult<()> {
    execute_script_in_with_options(lua, data_root, requested, environment, true, false)
}

pub(crate) fn execute_script_in_with_options(
    lua: &Lua,
    data_root: &Path,
    requested: &str,
    environment: mlua::Table,
    decrypt_persistent: bool,
    decompress: bool,
) -> LuaResult<()> {
    let path = resolve_script(data_root, requested).map_err(runtime_error)?;
    let mut bytes = fs::read(&path).map_err(runtime_error)?;
    if decrypt_persistent {
        bytes = decode_persistent_lua(bytes);
    }
    if decompress && bytes.starts_with(&stella_assets::SEVEN_Z_SIGNATURE) {
        bytes = stella_assets::unpack_7z(&bytes)
            .map_err(runtime_error)?
            .into_iter()
            .next()
            .ok_or_else(|| runtime_error("Lua archive contains no files"))?
            .bytes;
    }
    let prepared = prepare_lua_chunk(&bytes).map_err(runtime_error)?;
    lua.load(&prepared)
        .set_name(path.to_string_lossy())
        .set_environment(environment)
        .exec()
}

pub(crate) fn prepare_lua_chunk(bytes: &[u8]) -> Result<Vec<u8>, stella_assets::AssetError> {
    // Lua 5.1's native loader accepts both source and binary chunks. Purple's
    // bundled bytecode needs the 32-bit-number transcoder; editor/AppData
    // files may remain ordinary source text and must pass through unchanged.
    // The plain-runtime generator stores an already host-transcoded chunk in
    // a text envelope. Recover it before Lua evaluates the wrapper: object
    // environments need not publish implementation-only `table.concat`,
    // `loadstring`, `getfenv`, or `setfenv` helpers that the original stripped
    // chunk itself never referenced.
    if let Some(unwrapped) = stella_assets::lua::unwrap_host_chunk_text(bytes)? {
        return Ok(unwrapped);
    }
    if bytes.starts_with(&stella_assets::lua::LUA_SIGNATURE) {
        stella_assets::lua::prepare_for_host(bytes)
    } else {
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_lua_chunk;

    #[test]
    fn plain_runtime_text_envelope_is_unwrapped_before_object_execution() {
        let lua = mlua::Lua::new();
        let bytecode = lua.load("return 42").into_function().unwrap().dump(true);
        let wrapped = stella_assets::lua::wrap_host_chunk_as_text(&bytecode, "@probe.lua").unwrap();

        let prepared = prepare_lua_chunk(wrapped.as_bytes()).unwrap();
        assert_eq!(prepared, bytecode);
        assert_eq!(lua.load(&prepared).eval::<i32>().unwrap(), 42);
    }
}
