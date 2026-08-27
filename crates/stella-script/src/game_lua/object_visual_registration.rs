//! Scene-object visual properties recovered from GameLua's render-object members.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let sprite_bridge = Arc::clone(&render);
    globals.set(
        "native_setSprite",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100089B74 consumes two exact strings. sub_10004C7FC updates
            // native sprite/resource batches and never reflects `sprite` into
            // objects.world.
            let name = native_required_string(&args, 0, "native_setSprite")?;
            let sprite = native_required_string(&args, 1, "native_setSprite")?;
            let (sprite_region, mut composite_sprite) = {
                let resources = resources.lock().expect("resource runtime lock poisoned");
                (
                    resources.active_atlas_catalog_region(&sprite, &data_root),
                    resources.active_bound_composite(&sprite),
                )
            };
            // A present empty composite vector is the deferred-host null
            // pointer sentinel: preserve callback submission, but never let
            // wgpu resolve this name against a resource loaded later.
            if sprite_region.is_none() && composite_sprite.is_none() {
                composite_sprite = Some(Arc::new(CompositeSpriteOwner::new(Vec::new())));
            }
            let mut bridge = sprite_bridge.lock().expect("render bridge lock poisoned");
            let (z_bucket, old_sheet_id) = bridge
                .scene
                .get(&name)
                .map(|object| {
                    (
                        native_fcvtzs_f32(object.z_order as f32),
                        native_scene_sheet_id(object),
                    )
                })
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            let new_sheet_id = sprite_region
                .as_ref()
                .map(|region| region.native_sheet_id)
                .or_else(|| composite_sprite.as_ref()?.first_native_sheet_id())
                .unwrap_or(0);
            bridge
                .scene_render_index
                .move_sheet(z_bucket, old_sheet_id, new_sheet_id, &name);
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            object.sprite = sprite.into();
            object.sprite_bound = true;
            object.sprite_region = sprite_region;
            object.composite_sprite = composite_sprite;
            Ok(())
        })?,
    )?;

    let alpha_bridge = Arc::clone(&render);
    globals.set(
        "setObjectAlpha",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000866F8 consumes the complete string/number pair before
            // dispatching sub_100044E60, crossing the float32 ABI on slot 2.
            let name = native_required_string(&args, 0, "setObjectAlpha")?;
            let alpha = f64::from(native_required_number(&args, 1, "setObjectAlpha")? as f32);
            let mut bridge = alpha_bridge.lock().expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            object.alpha = alpha;
            Ok(())
        })?,
    )?;

    let z_order_bridge = Arc::clone(&render);
    globals.set(
        "changeZOrder",
        lua.create_function(move |lua, args: MultiValue| {
            // Registration at 0x10002EEDC uses the same strict string/number
            // adapter as setObjectAlpha. sub_1000592C4 first resolves the
            // render object, then moves it between integer z-order buckets,
            // writes its reflected attribute, and finally stores the float.
            let name = native_required_string(&args, 0, "changeZOrder")?;
            let z_order = f64::from(native_required_number(&args, 1, "changeZOrder")? as f32);
            {
                let mut bridge = z_order_bridge.lock().expect("render bridge lock poisoned");
                let (old_z_bucket, sheet) = bridge
                    .scene
                    .get(&name)
                    .map(|object| {
                        (
                            native_fcvtzs_f32(object.z_order as f32),
                            native_scene_sheet_id(object),
                        )
                    })
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                let new_z_bucket = native_fcvtzs_f32(z_order as f32);
                bridge
                    .scene_render_index
                    .move_z(old_z_bucket, new_z_bucket, sheet, &name);
                let object = bridge
                    .scene
                    .get_mut(&name)
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                object.z_order = z_order;
            }
            if let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(name.as_str())? {
                entry.set("z_order", z_order)?;
            }
            Ok(())
        })?,
    )?;

    let visibility_bridge = Arc::clone(&render);
    globals.set(
        "setVisible",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000859F4 reads an exact STRING and BOOLEAN, then
            // sub_10004CB94 resolves the native object and writes only its
            // visibility byte at +0x14A. It does not reflect into Lua.
            let name = native_required_string(&args, 0, "setVisible")?;
            let visible = native_required_boolean(&args, 1, "setVisible")?;
            let mut bridge = visibility_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let object = bridge
                .scene
                .get_mut(&name)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
            object.visible = visible;
            Ok(())
        })?,
    )?;

    let visible_query_bridge = Arc::clone(&render);
    globals.set(
        "isVisible",
        lua.create_function(move |_, args: MultiValue| {
            // sub_10004CBB8 reads the same RenderObjectData+0x14A byte and
            // therefore shares getRenderObject's unknown-name exception.
            let name = native_required_string(&args, 0, "isVisible")?;
            visible_query_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .scene
                .get(&name)
                .map(|object| object.visible)
                .ok_or_else(|| runtime_error(format!("Missing object: {name}")))
        })?,
    )?;

    Ok(())
}
