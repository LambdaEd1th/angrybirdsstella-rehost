//! Screenshot sharing and asynchronous URL-request bridge.

use std::{
    collections::VecDeque,
    io::Read,
    sync::{Arc, Mutex},
};

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::{RenderBridge, ScreenshotShareRequest, native_required_string, runtime_error};

// Purple stores this signed 32-bit sequence beside the process-global unique
// shader counter, not in GameLua or RenderBridge. The value is incremented
// before formatting and streamed through ostream's signed-int overload.
static SCREENSHOT_SEQUENCE: Mutex<i32> = Mutex::new(0);

const URL_CALLBACK_REGISTRY_KEY: &str = "stella.native_start_url_thread.callback";

#[derive(Debug)]
pub(crate) struct UrlRequestCompletion {
    url: String,
    body: Vec<u8>,
}

/// Cross-thread half of GameLua's `+0x660` URL worker and the process-global
/// zero-delay event queue used to hand successful responses back to Lua.
#[derive(Clone, Debug, Default)]
pub(crate) struct UrlRequestRuntime {
    completions: Arc<Mutex<VecDeque<UrlRequestCompletion>>>,
}

impl UrlRequestRuntime {
    fn pop_completion(&self) -> Option<UrlRequestCompletion> {
        self.completions
            .lock()
            .expect("URL completion queue lock poisoned")
            .pop_front()
    }
}

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
            let sequence = {
                let mut sequence = SCREENSHOT_SEQUENCE
                    .lock()
                    .expect("screenshot sequence lock poisoned");
                *sequence = sequence.wrapping_add(1);
                *sequence
            };
            let mut bridge = share_bridge.lock().expect("render bridge lock poisoned");
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

#[cfg(test)]
pub(crate) fn set_screenshot_sequence_for_test(sequence: i32) {
    *SCREENSHOT_SEQUENCE
        .lock()
        .expect("screenshot sequence lock poisoned") = sequence;
}

pub(super) fn install_url(
    lua: &Lua,
    globals: &Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<UrlRequestRuntime> {
    let runtime = UrlRequestRuntime::default();
    let url_bridge = Arc::clone(render);
    let completion_queue = Arc::clone(&runtime.completions);
    globals.set(
        "native_startURLThread",
        lua.create_function(move |lua, args: MultiValue| {
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

            // sub_100032688 overwrites GameLua+0x50 before constructing the
            // worker. All queued completion events consequently resolve the
            // callback that is live when the event is dispatched.
            lua.set_named_registry_value(URL_CALLBACK_REGISTRY_KEY, callback)?;
            url_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .requested_url = Some(url.clone());

            let completion_queue = Arc::clone(&completion_queue);
            std::thread::Builder::new()
                .name("stella-url-request".to_owned())
                .spawn(move || {
                    let Some(body) = fetch_http_200(&url) else {
                        return;
                    };
                    completion_queue
                        .lock()
                        .expect("URL completion queue lock poisoned")
                        .push_back(UrlRequestCompletion { url, body });
                })
                .map_err(|_| runtime_error("Creating thread failed"))?;
            Ok(())
        })?,
    )?;
    Ok(runtime)
}

/// Read the complete body accepted by Purple's `net::HttpFileInputStream`.
/// Both native URL-worker families use this constructor, whose post-redirect
/// status contract is exactly 200 rather than the complete 2xx range.
pub(super) fn fetch_http_200(url: &str) -> Option<Vec<u8>> {
    let Ok(mut response) = ureq::get(url).call() else {
        return None;
    };
    if response.status().as_u16() != 200 {
        return None;
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .ok()?;
    Some(body)
}

/// Execute the zero-delay events queued by completed URL workers. Purple's
/// AppController drains this scheduler before calling the application/GameLua
/// update virtual, so this must run at the head of [`StellaLua::update`].
pub(crate) fn dispatch_url_completions(lua: &Lua, runtime: &UrlRequestRuntime) -> LuaResult<()> {
    while let Some(completion) = runtime.pop_completion() {
        let callback: Function = lua.named_registry_value(URL_CALLBACK_REGISTRY_KEY)?;
        let url = lua.create_string(completion.url.as_bytes())?;
        let body = lua.create_string(&completion.body)?;
        callback.call::<()>((url, body))?;
    }
    Ok(())
}
