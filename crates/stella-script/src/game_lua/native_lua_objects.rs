//! Retained `lua::LuaObject` table identities embedded in GameLua.

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::game_environment;

#[derive(Debug, Clone, Copy)]
pub(crate) enum NativeLuaObject {
    KeyPressed,
    KeyReleased,
    KeyHold,
    Cursor,
    MultitouchSweep,
    MultitouchZoom,
    Objects,
    BlockTable,
    WorldAttributes,
    DeadBlocks,
    ClippedText,
    BlockEditorTable,
}

impl NativeLuaObject {
    fn field(self) -> &'static str {
        match self {
            Self::KeyPressed => "keyPressed",
            Self::KeyReleased => "keyReleased",
            Self::KeyHold => "keyHold",
            Self::Cursor => "cursor",
            Self::MultitouchSweep => "multitouchSweep",
            Self::MultitouchZoom => "multitouchZoom",
            Self::Objects => "objects",
            Self::BlockTable => "blockTable",
            Self::WorldAttributes => "worldAttributes",
            Self::DeadBlocks => "deadBlocks",
            Self::ClippedText => "clippedText",
            Self::BlockEditorTable => "blockEditorTable",
        }
    }

    fn registry_key(self) -> &'static str {
        match self {
            Self::KeyPressed => "stella.native_lua_object.key_pressed",
            Self::KeyReleased => "stella.native_lua_object.key_released",
            Self::KeyHold => "stella.native_lua_object.key_hold",
            Self::Cursor => "stella.native_lua_object.cursor",
            Self::MultitouchSweep => "stella.native_lua_object.multitouch_sweep",
            Self::MultitouchZoom => "stella.native_lua_object.multitouch_zoom",
            Self::Objects => "stella.native_lua_object.objects",
            Self::BlockTable => "stella.native_lua_object.block_table",
            Self::WorldAttributes => "stella.native_lua_object.world_attributes",
            Self::DeadBlocks => "stella.native_lua_object.dead_blocks",
            Self::ClippedText => "stella.native_lua_object.clipped_text",
            Self::BlockEditorTable => "stella.native_lua_object.block_editor_table",
        }
    }

    fn bound_registry_key(self) -> &'static str {
        match self {
            Self::KeyPressed => "stella.native_lua_object.key_pressed.bound",
            Self::KeyReleased => "stella.native_lua_object.key_released.bound",
            Self::KeyHold => "stella.native_lua_object.key_hold.bound",
            Self::Cursor => "stella.native_lua_object.cursor.bound",
            Self::MultitouchSweep => "stella.native_lua_object.multitouch_sweep.bound",
            Self::MultitouchZoom => "stella.native_lua_object.multitouch_zoom.bound",
            Self::Objects => "stella.native_lua_object.objects.bound",
            Self::BlockTable => "stella.native_lua_object.block_table.bound",
            Self::WorldAttributes => "stella.native_lua_object.world_attributes.bound",
            Self::DeadBlocks => "stella.native_lua_object.dead_blocks.bound",
            Self::ClippedText => "stella.native_lua_object.clipped_text.bound",
            Self::BlockEditorTable => "stella.native_lua_object.block_editor_table.bound",
        }
    }
}

pub(crate) fn retain_native_lua_object(
    lua: &Lua,
    object: NativeLuaObject,
    table: Option<&Table>,
) -> LuaResult<()> {
    match table {
        Some(table) => lua.set_named_registry_value(object.registry_key(), table.clone()),
        None => lua.set_named_registry_value(object.registry_key(), Value::Nil),
    }?;
    lua.set_named_registry_value(object.bound_registry_key(), true)
}

/// Return the native object's retained table. Pre-boot/unit-test runtimes do
/// not execute the constructor's script-loading phase, so their first native
/// use captures the corresponding game-environment field lazily.
pub(crate) fn native_lua_object(lua: &Lua, object: NativeLuaObject) -> LuaResult<Option<Table>> {
    if matches!(
        lua.named_registry_value::<Value>(object.bound_registry_key())?,
        Value::Boolean(true)
    ) {
        return match lua.named_registry_value::<Value>(object.registry_key())? {
            Value::Table(table) => Ok(Some(table)),
            _ => Ok(None),
        };
    }
    let environment = game_environment(lua)?;
    let table = match environment.get::<Value>(object.field())? {
        Value::Table(table) => Some(table),
        _ => None,
    };
    retain_native_lua_object(lua, object, table.as_ref())?;
    Ok(table)
}

pub(crate) fn retain_constructor_lua_objects(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for object in [NativeLuaObject::Objects, NativeLuaObject::BlockTable] {
        let table = match environment.get::<Value>(object.field())? {
            Value::Table(table) => Some(table),
            _ => None,
        };
        retain_native_lua_object(lua, object, table.as_ref())?;
    }
    Ok(())
}
