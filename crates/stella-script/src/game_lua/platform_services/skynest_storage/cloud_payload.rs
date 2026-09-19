//! PurpleState is native Lua assignment text, not JSON.
//!
//! Native 0x1000BA1F0 calls the same 0x10052A8FC table serializer used by
//! AppData files. Remote reads use an isolated, bounded, text-only Lua VM;
//! only plain data is copied into the game VM, never remote closures.

use crate::{Lua, LuaResult, Value, runtime_error};
use mlua::{HookTriggers, LuaOptions, StdLib, Table, VmState, chunk::ChunkMode};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_VM_BYTES: usize = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: usize = 100_000;
const INVALID: &str = "Invalid cloud settings data";

pub(super) fn encode(table: &Table) -> LuaResult<String> {
    let bytes = crate::game_lua::persistence::serialize_table(table)?;
    if bytes.len() > MAX_BYTES {
        return Err(runtime_error("Cloud settings exceed host size limit"));
    }
    // The serializer escapes non-ASCII string bytes, including embedded NULs.
    String::from_utf8(bytes).map_err(|_| runtime_error(INVALID))
}

pub(super) fn decode(lua: &Lua, source: &str) -> LuaResult<Table> {
    decode_inner(lua, source).map_err(|_| runtime_error(INVALID))
}

fn decode_inner(lua: &Lua, source: &str) -> LuaResult<Table> {
    if source.len() > MAX_BYTES {
        return Err(runtime_error(INVALID));
    }
    let isolated = Lua::new_with(StdLib::NONE, LuaOptions::default())?;
    isolated.set_memory_limit(MAX_VM_BYTES)?;
    let instructions = Arc::new(AtomicU32::new(0));
    isolated.set_hook(
        HookTriggers::new().every_nth_instruction(1000),
        move |_, _| {
            if instructions.fetch_add(1000, Ordering::Relaxed) >= 1_000_000 {
                return Err(runtime_error(INVALID));
            }
            Ok(VmState::Continue)
        },
    )?;
    let environment = isolated.create_table()?;
    isolated
        .load(source)
        .set_name("cloud settings")
        .set_mode(ChunkMode::Text)
        .set_environment(environment.clone())
        .exec()?;
    let mut budget = CopyBudget {
        entries: MAX_ENTRIES,
        bytes: MAX_BYTES,
        stack: BTreeSet::new(),
    };
    copy_table(lua, &environment, 0, &mut budget)
}

struct CopyBudget {
    entries: usize,
    bytes: usize,
    stack: BTreeSet<usize>,
}

fn copy_table(lua: &Lua, table: &Table, depth: usize, budget: &mut CopyBudget) -> LuaResult<Table> {
    if depth > MAX_DEPTH || table.metatable().is_some() {
        return Err(runtime_error(INVALID));
    }
    let identity = table.to_pointer() as usize;
    if !budget.stack.insert(identity) {
        return Err(runtime_error(INVALID));
    }
    let copy = lua.create_table()?;
    for entry in table.clone().pairs::<Value, Value>() {
        let (key, value) = entry?;
        budget.entries = budget
            .entries
            .checked_sub(1)
            .ok_or_else(|| runtime_error(INVALID))?;
        // Native serialized table keys are scalar; remote tables/closures must
        // not become keys, executable values or object references in the game.
        let key = copy_scalar(lua, key, budget)?;
        let value = match value {
            Value::Table(table) => Value::Table(copy_table(lua, &table, depth + 1, budget)?),
            value => copy_scalar(lua, value, budget)?,
        };
        copy.raw_set(key, value)?;
    }
    budget.stack.remove(&identity);
    Ok(copy)
}

fn copy_scalar(lua: &Lua, value: Value, budget: &mut CopyBudget) -> LuaResult<Value> {
    match value {
        Value::Nil | Value::Boolean(_) | Value::Integer(_) | Value::Number(_) => Ok(value),
        Value::String(value) => {
            let bytes = value.as_bytes();
            budget.bytes = budget
                .bytes
                .checked_sub(bytes.len())
                .ok_or_else(|| runtime_error(INVALID))?;
            Ok(Value::String(lua.create_string(bytes)?))
        }
        _ => Err(runtime_error(INVALID)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_payload_serializes_native_assignments_and_preserves_lua_data() {
        let lua = Lua::new();
        let source: Table = lua
            .load(
                r#"return {
            coins=777, chapter=2, nested={true,false,[4]="sparse",[false]="bool-key"},
            text="中文", bytes="a\000b\2557", inf=1/0, nan=0/0,
            skipped=function() end, _G="reserved", [5]="skip root number"
        }"#,
            )
            .eval()
            .unwrap();
        let encoded = encode(&source).unwrap();
        assert!(encoded.contains("coins = 777\n"));
        assert!(!encoded.starts_with('{'));
        assert!(!encoded.contains("skip root") && !encoded.contains("reserved"));
        let restored = decode(&lua, &encoded).unwrap();
        assert_eq!(restored.get::<i64>("coins").unwrap(), 777);
        assert_eq!(restored.get::<String>("text").unwrap(), "中文");
        assert_eq!(
            restored
                .get::<mlua::LuaString>("bytes")
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"a\0b\xff7"
        );
        assert!(restored.get::<f64>("inf").unwrap().is_infinite());
        assert!(restored.get::<f64>("nan").unwrap().is_nan());
        let nested: Table = restored.get("nested").unwrap();
        assert!(nested.get::<bool>(1).unwrap());
        assert_eq!(nested.get::<String>(4).unwrap(), "sparse");
        assert_eq!(nested.get::<String>(false).unwrap(), "bool-key");
        assert!(restored.get::<Value>("skipped").unwrap().is_nil());
    }

    #[test]
    fn cloud_payload_rejects_json_invalid_text_and_binary_chunks_without_echo() {
        let lua = Lua::new();
        for source in [
            r#"{"coins":777}"#,
            "synthetic_private_payload???",
            "\u{1b}Lua",
        ] {
            assert_eq!(
                decode(&lua, source).unwrap_err().to_string(),
                format!("runtime error: {INVALID}")
            );
        }
        assert_eq!(decode(&lua, "").unwrap().pairs::<Value, Value>().count(), 0);
    }

    #[test]
    fn cloud_payload_never_imports_remote_code_or_changes_game_globals() {
        let lua = Lua::new();
        lua.globals().set("cloud_sentinel", "original").unwrap();
        for source in [
            "_G.cloud_sentinel='changed'",
            "os.execute('synthetic-no-op')",
            "value = function() return 'remote' end",
            "value = {}; value.self = value",
            "value = {}; value[value] = true",
        ] {
            assert!(decode(&lua, source).is_err());
        }
        let data = decode(&lua, "cloud_sentinel='remote data'").unwrap();
        assert_eq!(data.get::<String>("cloud_sentinel").unwrap(), "remote data");
        assert_eq!(
            lua.globals().get::<String>("cloud_sentinel").unwrap(),
            "original"
        );
    }

    #[test]
    fn cloud_payload_bounds_vm_work_memory_and_table_depth() {
        let lua = Lua::new();
        for source in [
            "while true do end",
            "value='x'; for i=1,32 do value=value..value end",
            "value={}; local t=value; for i=1,100 do t.next={}; t=t.next end",
            "value={}; for i=1,100001 do value[i]=i end",
        ] {
            assert!(decode(&lua, source).is_err());
        }
        assert!(decode(&lua, &" ".repeat(MAX_BYTES + 1)).is_err());
    }

    #[test]
    fn cloud_payload_shared_tables_are_data_copied_but_cycles_are_rejected() {
        let lua = Lua::new();
        let table = decode(&lua, "local t={coins=7}; first=t; second=t").unwrap();
        let first: Table = table.get("first").unwrap();
        let second: Table = table.get("second").unwrap();
        assert_eq!(first.get::<i64>("coins").unwrap(), 7);
        assert_eq!(second.get::<i64>("coins").unwrap(), 7);
        assert_ne!(first.to_pointer(), second.to_pointer());
        let cyclic: Table = lua.load("local t={}; t.self=t; return t").eval().unwrap();
        assert!(encode(&cyclic).is_err());
    }
}
