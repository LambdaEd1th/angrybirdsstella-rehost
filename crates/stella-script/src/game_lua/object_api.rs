//! GameLua adapters for native scene-object mutation and inspection.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    install_object_transform_bindings(lua, globals, Arc::clone(&render))?;

    install_object_physics_bindings(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resources),
        Arc::clone(&data_root),
    )?;

    install_object_feature_bindings(lua, globals, Arc::clone(&render), draw_callbacks)?;

    install_object_visual_bindings(lua, globals, Arc::clone(&render), resources, data_root)?;

    install_object_query_bindings(lua, globals, Arc::clone(&render))?;
    let background_render_bridge = Arc::clone(&render);
    globals.set(
        "setBGColor",
        lua.create_function(move |_, args: MultiValue| {
            let channel = |index| -> LuaResult<u8> {
                // sub_100089A44 converts every Lua slot to float32 before
                // sub_100030C60 applies FMAX(0), FCVTZS and the explicit
                // 255 ceiling. Rust's saturating float-to-u8 conversion has
                // those same negative/NaN/overflow results once the native
                // float32 boundary has been observed.
                Ok(native_required_number(&args, index, "setBGColor")? as f32 as u8)
            };
            let color = [channel(0)?, channel(1)?, channel(2)?];
            background_render_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .background_color = color;
            Ok(())
        })?,
    )?;
    let background_render_bridge = Arc::clone(&render);
    globals.set(
        "getBGColor",
        lua.create_function(move |_, ()| {
            let color = background_render_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .background_color;
            Ok((
                f64::from(color[0]),
                f64::from(color[1]),
                f64::from(color[2]),
            ))
        })?,
    )?;
    Ok(())
}

/// Return the canonical `objects.world` table, creating the two native-owned
/// containers when a script has not initialized them yet.
pub(crate) fn object_world(lua: &Lua) -> LuaResult<mlua::Table> {
    let environment = game_environment(lua)?;
    let objects = match native_lua_object(lua, NativeLuaObject::Objects)? {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            environment.set("objects", table.clone())?;
            retain_native_lua_object(lua, NativeLuaObject::Objects, Some(&table))?;
            table
        }
    };
    match objects.get::<Value>("world")? {
        Value::Table(table) => Ok(table),
        _ => {
            let table = lua.create_table()?;
            objects.set("world", table.clone())?;
            Ok(table)
        }
    }
}
