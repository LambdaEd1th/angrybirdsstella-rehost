//! Retained `lua::LuaObject` table identities embedded in GameLua.

use mlua::{Lua, RegistryKey, Result as LuaResult, Table, Value};

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
    const COUNT: usize = 12;

    fn index(self) -> usize {
        match self {
            Self::KeyPressed => 0,
            Self::KeyReleased => 1,
            Self::KeyHold => 2,
            Self::Cursor => 3,
            Self::MultitouchSweep => 4,
            Self::MultitouchZoom => 5,
            Self::Objects => 6,
            Self::BlockTable => 7,
            Self::WorldAttributes => 8,
            Self::DeadBlocks => 9,
            Self::ClippedText => 10,
            Self::BlockEditorTable => 11,
        }
    }

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
}

/// Rust-side layout companion to GameLua's fixed `lua::LuaObject` members.
///
/// `RegistryKey` is the integer registry reference itself, so resolving one
/// slot performs the same direct indexed registry load as the native wrapper.
/// `Some(LUA_REFNIL)` distinguishes a deliberately retained nil from an
/// uninitialized slot without a second flag or named-registry lookup.
struct NativeLuaObjectStore {
    slots: [Option<RegistryKey>; NativeLuaObject::COUNT],
}

impl Default for NativeLuaObjectStore {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

pub(crate) fn retain_native_lua_object(
    lua: &Lua,
    object: NativeLuaObject,
    table: Option<&Table>,
) -> LuaResult<()> {
    let value = table.cloned().map(Value::Table).unwrap_or(Value::Nil);
    if let Some(mut store) = lua.app_data_mut::<NativeLuaObjectStore>()
        && let Some(key) = store.slots[object.index()].as_mut()
    {
        return lua.replace_registry_value(key, value);
    }

    let key = lua.create_registry_value(value)?;
    if lua.app_data_ref::<NativeLuaObjectStore>().is_none() {
        lua.set_app_data(NativeLuaObjectStore::default());
    }
    lua.app_data_mut::<NativeLuaObjectStore>()
        .expect("native Lua-object store must be installed")
        .slots[object.index()] = Some(key);
    Ok(())
}

/// Return the native object's retained table. Pre-boot/unit-test runtimes do
/// not execute the constructor's script-loading phase, so their first native
/// use captures the corresponding game-environment field lazily.
pub(crate) fn native_lua_object(lua: &Lua, object: NativeLuaObject) -> LuaResult<Option<Table>> {
    if let Some(store) = lua.app_data_ref::<NativeLuaObjectStore>()
        && let Some(key) = &store.slots[object.index()]
    {
        return match lua.registry_value::<Value>(key)? {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> (Lua, Table) {
        let lua = Lua::new();
        let environment = lua.create_table().unwrap();
        lua.globals().set("gamelua", environment.clone()).unwrap();
        (lua, environment)
    }

    #[test]
    fn retained_slot_uses_direct_registry_identity_after_global_shadowing() {
        let (lua, environment) = runtime();
        let original = lua.create_table().unwrap();
        environment.set("objects", original.clone()).unwrap();
        let retained = native_lua_object(&lua, NativeLuaObject::Objects)
            .unwrap()
            .unwrap();
        assert_eq!(retained.to_pointer(), original.to_pointer());

        let shadow = lua.create_table().unwrap();
        environment.set("objects", shadow).unwrap();
        let retained = native_lua_object(&lua, NativeLuaObject::Objects)
            .unwrap()
            .unwrap();
        assert_eq!(retained.to_pointer(), original.to_pointer());
    }

    #[test]
    fn retained_nil_does_not_lazily_capture_a_later_global() {
        let (lua, environment) = runtime();
        retain_native_lua_object(&lua, NativeLuaObject::WorldAttributes, None).unwrap();
        environment
            .set("worldAttributes", lua.create_table().unwrap())
            .unwrap();
        assert!(
            native_lua_object(&lua, NativeLuaObject::WorldAttributes)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn retained_slot_can_replace_nil_and_table_values() {
        let (lua, _) = runtime();
        retain_native_lua_object(&lua, NativeLuaObject::DeadBlocks, None).unwrap();
        let replacement = lua.create_table().unwrap();
        retain_native_lua_object(&lua, NativeLuaObject::DeadBlocks, Some(&replacement)).unwrap();
        let retained = native_lua_object(&lua, NativeLuaObject::DeadBlocks)
            .unwrap()
            .unwrap();
        assert_eq!(retained.to_pointer(), replacement.to_pointer());

        retain_native_lua_object(&lua, NativeLuaObject::DeadBlocks, None).unwrap();
        assert!(
            native_lua_object(&lua, NativeLuaObject::DeadBlocks)
                .unwrap()
                .is_none()
        );
    }
}
