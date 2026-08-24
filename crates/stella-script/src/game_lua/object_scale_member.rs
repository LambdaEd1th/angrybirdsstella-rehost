//! Ordinary visual-scale member (`sub_100040304`, 188 bytes/1 block).

use crate::*;

pub(super) fn apply(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    scale_x: f64,
    scale_y: f64,
) -> LuaResult<()> {
    // The native member requires objects.world[name] to be a table, mirrors
    // both float fields, and only then performs the throwing render lookup.
    let entry = match object_world(lua)?.raw_get::<Value>(name)? {
        Value::Table(entry) => entry,
        _ => {
            return Err(runtime_error(format!(
                "setScale objects.world entry missing: {name}"
            )));
        }
    };
    entry.set("scaleX", scale_x)?;
    entry.set("scaleY", scale_y)?;

    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let object = bridge
        .scene
        .get_mut(name)
        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
    // Purple stores the live pair at RenderObjectData+0xBC/+0xC0 and the
    // persistent bounce base at +0xCC/+0xD0. sub_10005E898 reads the latter
    // and overwrites only the former, so both pairs must remain independent.
    object.scale_x = scale_x;
    object.scale_y = scale_y;
    object.base_scale_x = scale_x;
    object.base_scale_y = scale_y;
    Ok(())
}
