//! No-op layer, platform video, z-range, and scene callback adapters.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    // Generated adapter sub_100088D24 strictly reads slot 1 as NUMBER before
    // dispatching to nullsub_13/sub_10004C4B8. The body is empty, but the Lua
    // type contract and zero-result ABI are still observable.
    globals.set(
        "drawLayer",
        lua.create_function(|_, args: MultiValue| {
            native_required_number(&args, 0, "drawLayer")?;
            Ok(())
        })?,
    )?;
    let video_bridge = Arc::clone(&render);
    globals.set(
        "playVideo",
        lua.create_function(move |_, args: MultiValue| {
            let path = native_required_string(&args, 0, "playVideo")?;
            // sub_100051B60 forwards the path into the platform video service.
            video_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .requested_video = Some(path);
            Ok(())
        })?,
    )?;

    let z_range_bridge = Arc::clone(&render);
    globals.set(
        "native_setZOrderRange",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100084DA8 requires two NUMBERs, narrows to float32 and
            // applies FCVTZS before calling the 12-byte sub_10004BAA8.
            let minimum = value_number_at(&args, 0).ok_or_else(|| {
                LuaError::RuntimeError(
                    "bad argument #1 to 'native_setZOrderRange' (number expected)".to_owned(),
                )
            })? as f32;
            let maximum = value_number_at(&args, 1).ok_or_else(|| {
                LuaError::RuntimeError(
                    "bad argument #2 to 'native_setZOrderRange' (number expected)".to_owned(),
                )
            })? as f32;
            let mut bridge = z_range_bridge.lock().expect("render bridge lock poisoned");
            bridge.z_order_min = f64::from(native_fcvtzs_f32(minimum));
            bridge.z_order_max = f64::from(native_fcvtzs_f32(maximum));
            Ok(())
        })?,
    )?;
    for (function_name, pre_draw) in [
        ("native_setPreDrawFunction", true),
        ("native_setPostDrawFunction", false),
    ] {
        let callback_store = Rc::clone(&draw_callbacks);
        globals.set(
            function_name,
            lua.create_function(move |lua, args: MultiValue| {
                // Hand-written members sub_10004E3C0/sub_10004E570 guard
                // slot 1 as STRING. A missing or nil slot 2 clears; every
                // other value must pass the FUNCTION guard transactionally.
                let name = native_required_string(&args, 0, function_name)?;
                let function = match args.iter().nth(1) {
                    None | Some(Value::Nil) => None,
                    Some(Value::Function(function)) => Some(function.clone()),
                    Some(_) => {
                        return Err(LuaError::RuntimeError(format!(
                            "bad argument #2 to '{function_name}' (function expected)"
                        )));
                    }
                };
                let mut callbacks = callback_store.borrow_mut();
                let world_identity = object_world(lua)?.to_pointer() as usize;
                if callbacks.object_world_identity != Some(world_identity) {
                    callbacks.records.clear();
                    callbacks.object_world_identity = Some(world_identity);
                }
                // sub_10004E3C0/sub_10004E570 call getRenderObject before
                // touching the callback holder. A missing name raises the same
                // native error instead of creating a detached callback entry.
                let Some(record) = callbacks.records.get_mut(&name) else {
                    return Err(LuaError::RuntimeError(format!("Missing object: {name}")));
                };
                let target = if pre_draw {
                    &mut record.pre
                } else {
                    &mut record.post
                };
                *target = function;
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
