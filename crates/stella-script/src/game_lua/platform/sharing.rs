//! Screenshot sharing and asynchronous URL-request bridge.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::{RenderBridge, ScreenshotShareRequest, native_required_string, runtime_error};

pub(super) fn install_screenshot(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let share_bridge = Arc::clone(render);
    globals.set(
        "native_shareScreenShot",
        lua.create_function(move |_, args: MultiValue| {
            let title = native_required_string(&args, 0, "native_shareScreenShot")?;
            let mut bridge = share_bridge.lock().expect("render bridge lock poisoned");
            bridge.screenshot_sequence = bridge.screenshot_sequence.wrapping_add(1);
            let sequence = bridge.screenshot_sequence;
            bridge
                .screenshot_share_requests
                .push(ScreenshotShareRequest {
                    sequence,
                    filename: format!("Stella_Screenshot{sequence}.png"),
                    title,
                });
            Ok(())
        })?,
    )?;

    Ok(())
}

pub(super) fn install_url(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let url_bridge = Arc::clone(render);
    let url_callback = Rc::new(RefCell::new(None::<Function>));
    globals.set(
        "native_startURLThread",
        lua.create_function(move |_, args: MultiValue| {
            let url = native_required_string(&args, 0, "native_startURLThread")?;
            let callback = match args.iter().nth(1) {
                Some(Value::Function(callback)) => callback.clone(),
                _ => {
                    return Err(runtime_error(
                        "bad argument #2 to 'native_startURLThread' (function expected)",
                    ));
                }
            };
            if args.len() == 3 && !matches!(args.iter().nth(2), Some(Value::Boolean(_))) {
                return Err(runtime_error(
                    "bad argument #3 to 'native_startURLThread' (boolean expected)",
                ));
            }
            *url_callback.borrow_mut() = Some(callback);
            url_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .requested_url = Some(url);
            Ok(())
        })?,
    )?;
    Ok(())
}
