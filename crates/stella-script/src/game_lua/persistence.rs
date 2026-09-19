//! AppData file adapters and native Lua 5.1 serialization facade.

mod files;
mod messages;
mod serialization;

pub(crate) use serialization::serialize_table;

pub(crate) use files::{
    decode_persistent_lua, install_persistent_save, install_table_files, load_persistent_lua_table,
    load_saved_lua_table, write_saved_lua_table,
};
pub(crate) use messages::{deliver_persistent_load_messages, pending_persistent_load_messages};
