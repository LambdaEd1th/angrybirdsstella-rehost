//! Particle-table spawn entry; native named method registers at `0x10002F6D8`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let particles: mlua::Table = globals.get("particles")?;
    let traced_definitions = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    particles.set(
        "native_addParticlesWithMode",
        lua.create_function(move |lua, args: MultiValue| {
            trace_call(
                lua,
                "native_addParticlesWithMode",
                &traced_definitions,
                &args,
            );
            // Hand-written member `sub_10008E524` constructs its table
            // wrapper from Lua stack slot -1.  Additional leading values are
            // therefore ignored, while a missing/non-table top value throws.
            let index = args
                .len()
                .checked_sub(1)
                .ok_or_else(|| runtime_error("native_addParticlesWithMode expects a table"))?;
            let query = native_required_table(&args, index, "native_addParticlesWithMode")?;
            // ParticleData resolves AtlasSprite/CompoSprite before the
            // virtual add call and retains those pointers for its lifetime.
            // Preserve native lock ordering: ResourceManager, then bridge.
            let resources = resources.lock().expect("resource runtime lock poisoned");
            spawn_particles(
                lua,
                &mut render.lock().expect("render bridge lock poisoned"),
                &query,
                &resources,
                &data_root,
            )?;
            Ok(MultiValue::new())
        })?,
    )?;
    Ok(())
}

fn trace_call(
    lua: &Lua,
    method_name: &str,
    traced_definitions: &Arc<Mutex<BTreeSet<String>>>,
    args: &MultiValue,
) {
    if std::env::var_os("STELLA_TRACE_PARTICLES").is_none() {
        return;
    }
    let rendered = args
        .iter()
        .map(|value| {
            lua.from_value::<serde_json::Value>(value.clone())
                .ok()
                .and_then(|json| serde_json::to_string(&json).ok())
                .unwrap_or_else(|| describe_value(value))
        })
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("particle-api {method_name}({rendered})");

    let Some(definition_name) = args.iter().find_map(|value| match value {
        Value::Table(table) => table.get::<String>("definitionName").ok(),
        _ => None,
    }) else {
        return;
    };
    if !traced_definitions
        .lock()
        .expect("particle definition trace lock poisoned")
        .insert(definition_name.clone())
    {
        return;
    }
    let Ok(environment) = game_environment(lua) else {
        return;
    };
    let Ok(Value::Table(particle_table)) = environment.get::<Value>("particleTable") else {
        return;
    };
    let Ok(Value::Table(definitions)) = particle_table.get::<Value>("particles") else {
        return;
    };
    let Ok(value) = definitions.get::<Value>(definition_name.as_str()) else {
        return;
    };
    let Ok(json) = lua.from_value::<serde_json::Value>(value) else {
        return;
    };
    eprintln!(
        "particle-definition {definition_name}={}",
        serde_json::to_string(&json).unwrap_or_else(|_| "null".to_owned())
    );
}
