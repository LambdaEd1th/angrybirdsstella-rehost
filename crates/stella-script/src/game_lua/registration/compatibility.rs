//! Late utility members and the host's missing-global diagnostic boundary.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    missing: Arc<Mutex<BTreeSet<String>>>,
    fallback_calls: Arc<Mutex<BTreeSet<String>>>,
    compatibility_bindings: Arc<Mutex<BTreeSet<String>>>,
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
    globals.set(
        "uniqueDeviceId",
        // pf::DeviceID::Impl first tries the MAC address and, for iOS's
        // 02:00:00:00:00:00 sentinel, identifierForVendor. Its literal
        // result when neither platform identifier exists is "unavailable".
        lua.create_function(|_, ()| Ok("unavailable"))?,
    )?;
    install_unique_shaders(lua, globals)?;
    globals.set(
        "fileExistsInAppData",
        lua.create_function({
            let root = Arc::clone(&data_root);
            move |_, path: String| {
                Ok(app_data_path(&root, &path)
                    .map(|path| path.is_file())
                    .unwrap_or(false))
            }
        })?,
    )?;
    globals.set(
        "checkForLuaFile",
        lua.create_function({
            let root = Arc::clone(&data_root);
            move |_, path: String| Ok(resolve_script(&root, &path).is_ok())
        })?,
    )?;

    let metatable = lua.create_table()?;
    metatable.set(
        "__index",
        lua.create_function(move |_, (_table, key): (Value, Value)| {
            let key = match key {
                Value::String(value) => value.to_string_lossy(),
                value => format!("{value:?}"),
            };
            missing
                .lock()
                .expect("missing-global lock poisoned")
                .insert(key);
            Ok(Value::Nil)
        })?,
    )?;
    globals.set_metatable(Some(metatable))
}

fn install_unique_shaders(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let unique_shader_counter = Arc::new(Mutex::new(0_u32));
    let unique_shaders = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let create_shader_counter = Arc::clone(&unique_shader_counter);
    let create_shader_set = Arc::clone(&unique_shaders);
    globals.set(
        "createUniqueShaders",
        lua.create_function(move |lua, (base, count): (String, u32)| {
            let shaders = lua.create_table()?;
            let mut counter = create_shader_counter
                .lock()
                .expect("unique shader counter lock poisoned");
            let mut live = create_shader_set
                .lock()
                .expect("unique shader set lock poisoned");
            for index in 1..=count {
                // sub_10004E720 appends a process-global decimal counter and
                // advances it across separate calls.
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
            let mut live = destroy_shader_set
                .lock()
                .expect("unique shader set lock poisoned");
            for name in args.iter().filter_map(value_string) {
                live.remove(&name);
            }
            Ok(())
        })?,
    )
}
