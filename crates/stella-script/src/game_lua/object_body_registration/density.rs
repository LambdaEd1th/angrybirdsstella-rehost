//! Direct `native_setDensity` Lua member (`sub_100030D1C`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let density_bridge = Arc::clone(render);
    globals.set(
        "native_setDensity",
        lua.create_function(move |lua, args: MultiValue| {
            // The hand-written member reads the numeric stack top first and
            // the string immediately below it. Leading values are ignored.
            let last = args.len().saturating_sub(1);
            let density =
                f64::from(native_required_number(&args, last, "native_setDensity")? as f32);
            let name_index = args.len().saturating_sub(2);
            let name = native_required_string(&args, name_index, "native_setDensity")?;
            {
                let mut bridge = density_bridge.lock().expect("render bridge lock poisoned");
                let object = bridge
                    .scene
                    .get_mut(&name)
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                if !object.has_physics_body() || object.fixture_densities.is_empty() {
                    return Err(runtime_error(format!(
                        "Object has no fixture for native_setDensity: {name}"
                    )));
                }
                let old_center = object.world_center();
                *object
                    .fixture_densities
                    .last_mut()
                    .expect("fixture presence checked above") = density;
                object.reset_native_mass_data(old_center);
            }
            // The reflected Lua field is written only after ResetMassData.
            match object_world(lua)?.raw_get::<Value>(name.as_str())? {
                Value::Table(entry) => entry.set("density", density)?,
                _ => {
                    return Err(runtime_error(format!(
                        "native_setDensity objects.world entry missing: {name}"
                    )));
                }
            }
            Ok(())
        })?,
    )
}
