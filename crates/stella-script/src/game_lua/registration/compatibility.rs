//! Late utility members and the host's missing-global diagnostic boundary.

use std::collections::HashSet;

use mlua::LuaString;

use crate::*;

static UNIQUE_SHADER_COUNTER: Mutex<i32> = Mutex::new(0);

/// Diagnostic-only ownership for absent interned Lua string keys.
///
/// Lua 5.1 interns string keys, and the original global lookup simply returns
/// nil when the global table has no metatable. Retain each first-seen TString
/// so its pointer cannot be recycled, then keep repeated diagnostic probes on
/// the pointer-only path instead of rematerializing a Rust String and locking
/// the public report set every frame.
#[derive(Default)]
struct MissingStringCache {
    identities: HashSet<usize>,
    owners: Vec<LuaString>,
}

impl MissingStringCache {
    fn remember(&mut self, value: &LuaString) -> bool {
        let identity = value.to_pointer() as usize;
        if !self.identities.insert(identity) {
            return false;
        }
        self.owners.push(value.clone());
        true
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    missing: Arc<Mutex<BTreeSet<String>>>,
    fallback_calls: Arc<Mutex<BTreeSet<String>>>,
    compatibility_bindings: Arc<Mutex<BTreeSet<String>>>,
    track_missing_globals: bool,
) -> LuaResult<()> {
    for name in NATIVE_NOOP_FUNCTIONS {
        if globals.contains_key(*name)? {
            continue;
        }
        let function_name = (*name).to_owned();
        compatibility_bindings
            .lock()
            .expect("compatibility-binding lock poisoned")
            .insert(function_name.clone());
        let invoked_fallbacks = Arc::clone(&fallback_calls);
        globals.set(
            *name,
            lua.create_function(move |_, args: MultiValue| {
                invoked_fallbacks
                    .lock()
                    .expect("fallback-call lock poisoned")
                    .insert(function_name.clone());
                if std::env::var_os("STELLA_TRACE_NATIVE").is_some()
                    || std::env::var_os("STELLA_TRACE_NOOPS").is_some()
                {
                    let rendered = args
                        .iter()
                        .map(describe_value)
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!("native-noop {function_name}({rendered})");
                }
                Ok(MultiValue::new())
            })?,
        )?;
    }

    for (name, use_or) in [("performBitwiseAnd", false), ("performBitwiseOr", true)] {
        globals.set(
            name,
            lua.create_function(move |_, args: MultiValue| {
                // sub_100083874 narrows both NUMBER slots to float32. The
                // members at 0x100056964/0x100056978 then use FCVTZS W,
                // perform the signed 32-bit operation and SCVTF the result.
                let left = native_fcvtzs_f32(native_required_number(&args, 0, name)? as f32);
                let right = native_fcvtzs_f32(native_required_number(&args, 1, name)? as f32);
                let value = if use_or { left | right } else { left & right };
                Ok(value as f32)
            })?,
        )?;
    }
    install_unique_shaders(lua, globals)?;
    globals.set(
        "fileExistsInAppData",
        lua.create_function({
            let root = Arc::clone(&data_root);
            move |_, args: MultiValue| {
                // Shared generated boolean/string wrapper sub_100088F68
                // requires an exact STRING in slot one and ignores extras.
                let path = native_required_string(&args, 0, "fileExistsInAppData")?;
                Ok(app_data_path(&root, &path)
                    // `sub_100501CD0` is a direct `access(path, 0)` probe;
                    // unlike Rust's `is_file`, it also reports directories as
                    // existing.
                    .map(|path| path.exists())
                    .unwrap_or(false))
            }
        })?,
    )?;
    globals.set(
        "checkForLuaFile",
        lua.create_function({
            let root = Arc::clone(&data_root);
            move |_, args: MultiValue| {
                let path = native_required_string(&args, 0, "checkForLuaFile")?;
                Ok(resolve_script(&root, &path).is_ok())
            }
        })?,
    )?;

    if track_missing_globals {
        let metatable = lua.create_table()?;
        let missing_string_cache = Rc::new(RefCell::new(MissingStringCache::default()));
        metatable.set(
            "__index",
            lua.create_function(move |_, (_table, key): (Value, Value)| {
                let key = match key {
                    Value::String(value) => {
                        if !missing_string_cache.borrow_mut().remember(&value) {
                            return Ok(Value::Nil);
                        }
                        value.to_string_lossy()
                    }
                    value => format!("{value:?}"),
                };
                missing
                    .lock()
                    .expect("missing-global lock poisoned")
                    .insert(key);
                Ok(Value::Nil)
            })?,
        )?;
        globals.set_metatable(Some(metatable))?;
    }
    Ok(())
}

fn install_unique_shaders(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let unique_shaders = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let create_shader_set = Arc::clone(&unique_shaders);
    globals.set(
        "createUniqueShaders",
        lua.create_function(move |lua, args: MultiValue| {
            // Hand-written sub_10004E720 reads exact STRING/NUMBER slots,
            // narrows the count through float32 and FCVTZS, and ignores the
            // remaining Lua stack. dword_100C0FF88 is process-global.
            let base = native_required_string(&args, 0, "createUniqueShaders")?;
            let count =
                native_fcvtzs_f32(native_required_number(&args, 1, "createUniqueShaders")? as f32);
            let shaders = lua.create_table()?;
            let mut counter = UNIQUE_SHADER_COUNTER
                .lock()
                .expect("unique shader counter lock poisoned");
            let mut live = create_shader_set
                .lock()
                .expect("unique shader set lock poisoned");
            for index in 1..=count {
                let name = format!("{base}{}", *counter);
                *counter = counter.wrapping_add(1);
                live.insert(name.clone());
                shaders.raw_set(index, name)?;
            }
            Ok(shaders)
        })?,
    )?;
    let destroy_shader_set = Arc::clone(&unique_shaders);
    globals.set(
        "destroyUniqueShaders",
        lua.create_function(move |_, args: MultiValue| {
            // Generated sub_10008426C requires one exact TABLE. Member
            // sub_10004EC88 counts every key with lua_next, then (only when
            // count >= 2) visits raw integer indices [1, count), converting
            // each value with lua_tolstring and using an empty string when
            // that conversion fails.
            let shaders = native_required_table(&args, 0, "destroyUniqueShaders")?;
            let count = shaders
                .clone()
                .pairs::<Value, Value>()
                .try_fold(0_u32, |count, pair| pair.map(|_| count.wrapping_add(1)))?;
            let mut live = destroy_shader_set
                .lock()
                .expect("unique shader set lock poisoned");
            if count >= 2 {
                for index in 1..count {
                    let value = shaders.raw_get::<Value>(i64::from(index))?;
                    let name = native_lua51_string(&value).unwrap_or_default();
                    live.remove(&name);
                }
            }
            Ok(())
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_string_cache_deduplicates_interned_keys_and_retains_owners() {
        let lua = Lua::new();
        let first = lua.create_string("missing_name").unwrap();
        let same = lua.create_string("missing_name").unwrap();
        let other = lua.create_string("other_name").unwrap();
        let mut cache = MissingStringCache::default();

        assert!(cache.remember(&first));
        assert!(!cache.remember(&same));
        assert!(cache.remember(&other));
        assert_eq!(cache.identities.len(), 2);
        assert_eq!(cache.owners.len(), 2);
    }
}
