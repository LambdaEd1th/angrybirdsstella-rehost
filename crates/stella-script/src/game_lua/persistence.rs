//! AppData file adapters and native Lua 5.1 serialization facade.

mod files;
mod serialization;

pub(crate) use files::{
    decode_persistent_lua, install_persistent_save, install_table_files, load_saved_lua_table,
    write_saved_lua_table,
};
