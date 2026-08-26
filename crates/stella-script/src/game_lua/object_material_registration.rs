//! Scene-object material, texture, and water-property adapters.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let material_bridge = Arc::clone(&render);
    globals.set(
        "setMaterial",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setMaterial")?;
            let material = native_required_string(&args, 1, "setMaterial")?;
            let native_material = match material.as_str() {
                "wood" => 1,
                "stone" => 2,
                "glass" => 3,
                // sub_10004CBD0 compares all three literals before it ever
                // resolves the object. Every other string is a total no-op,
                // including when the named object does not exist.
                _ => return Ok(()),
            };
            let mut bridge = material_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // This enum at RenderObjectData+0x18 is distinct from the Lua
            // string read by the game-side collision-material filter.
            object.native_material = native_material;
            Ok(())
        })?,
    )?;

    let texture_bridge = Arc::clone(&render);
    globals.set(
        "setTexture",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setTexture")?;
            let texture = native_required_string(&args, 1, "setTexture")?;
            let texture_binding = resources
                .lock()
                .expect("resource runtime lock poisoned")
                .active_masked_texture_source(&texture, &data_root)
                .map_or(MaskedTextureBinding::Missing, MaskedTextureBinding::Source);
            let mut bridge = texture_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_10004CC74 writes the resource name at +0x70, resolves the
            // texture pointer into +0x80, and performs no Lua reflection.
            object.texture = Some(texture.into());
            object.texture_binding = Some(Arc::new(texture_binding));
            Ok(())
        })?,
    )?;

    let texture_scale_bridge = Arc::clone(&render);
    globals.set(
        "setTextureScale",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setTextureScale")?;
            let scale = f64::from(native_required_number(&args, 1, "setTextureScale")? as f32);
            let mut bridge = texture_scale_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_10004CE38 stores the generated adapter's float32 scalar at
            // RenderObjectData+0xC4 and performs no Lua-world reflection.
            object.texture_scale = scale;
            Ok(())
        })?,
    )?;

    let water_density_bridge = Arc::clone(&render);
    globals.set(
        "native_setWaterDensity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "native_setWaterDensity")?;
            let density =
                f64::from(native_required_number(&args, 1, "native_setWaterDensity")? as f32);
            let mut bridge = water_density_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_100059554 writes only RenderObjectData+0x150.
            object.water_density = density;
            Ok(())
        })?,
    )?;

    let is_water_bridge = Arc::clone(&render);
    globals.set(
        "native_setIsWater",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "native_setIsWater")?;
            let is_water = native_required_boolean(&args, 1, "native_setIsWater")?;
            let mut bridge = is_water_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            // sub_100059530 stores this byte at RenderObjectData+0x14B and
            // does not mirror it into objects.world.
            object.is_water = is_water;
            Ok(())
        })?,
    )?;

    Ok(())
}
