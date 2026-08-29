//! Object destruction and late flash members at separate constructor sites.

use crate::*;

pub(super) fn install_remove(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    globals.set(
        "removeObject",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "removeObject")?;
            let removed =
                remove_native_objects_with_joint_callbacks(lua, &render, vec![name.clone()])?;
            {
                let mut callbacks = draw_callbacks.borrow_mut();
                for removed_name in &removed {
                    callbacks.remove_record(removed_name);
                }
            }
            let world = object_world(lua)?;
            if removed.is_empty() {
                world.raw_set(name.as_str(), Value::Nil)?;
            } else {
                for removed_name in removed {
                    world.raw_set(removed_name.as_str(), Value::Nil)?;
                }
            }
            Ok(())
        })?,
    )
}

pub(super) fn install_flash(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for (function_name, enabled) in [("setFlashAnimation", true), ("removeFlashAnimation", false)] {
        let flash_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let name = native_required_string(&args, 0, function_name)?;
                if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                    eprintln!("native {function_name}({name:?})");
                }
                let mut bridge = flash_bridge.lock().expect("render bridge lock poisoned");
                let Some(object) = bridge.game_lua_object_mut(&name) else {
                    return Err(runtime_error(format!("Missing object: {name}")));
                };
                object.flash_animation = enabled;
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
