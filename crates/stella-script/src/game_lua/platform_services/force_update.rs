//! Discontinued cloud-version service boundary.

use crate::*;

const GAME_VERSION: &str = "1.1.6";

fn c_atoi(component: &str) -> i64 {
    // sub_100026330 delegates every dot-separated component to C `atoi`.
    // Version fields are tiny in shipped data; saturating here only makes the
    // otherwise-undefined overflow case deterministic on the Rust host.
    let bytes = component.as_bytes();
    let mut cursor = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let negative = match bytes.get(cursor) {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => {
            cursor += 1;
            false
        }
        _ => false,
    };
    let mut value = 0_i64;
    while let Some(byte) = bytes.get(cursor).copied().filter(u8::is_ascii_digit) {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
        cursor += 1;
    }
    if negative { -value } else { value }
}

fn version_at_least(current: &str, required: &str) -> bool {
    let current = current.split('.').map(c_atoi).collect::<Vec<_>>();
    let required = required.split('.').map(c_atoi).collect::<Vec<_>>();
    for (current, required) in current.iter().zip(&required) {
        match current.cmp(required) {
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Equal => {}
        }
    }
    // Purple does not pad a shorter version with zeroes: after an equal
    // common prefix, the version containing at least as many components wins.
    current.len() >= required.len()
}

fn should_force_update(document: &serde_json::Value) -> bool {
    if !document
        .get("isEnabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }

    // The iOS DeviceInfo implementation at sub_10053B294 reports a system
    // version and gates this comparison with minimumIOSVersionRequired. The
    // same native abstraction explicitly skips that gate when a platform has
    // no iOS-version provider. Desktop rehost backends take that latter path.
    let required = document
        .get("requiredVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    !version_at_least(GAME_VERSION, required)
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let force_update = lua.create_table()?;
    let check_root = Arc::clone(&data_root);
    force_update.set(
        "native_checkForcedUpdate",
        lua.create_function(move |_, args: MultiValue| {
            // Adapter sub_1000268E8 reads a required configuration string and
            // a required Lua callback before sub_1000256B4 performs the
            // version checks and ignores the tail. The member decrypts with
            // sub_1000E3A14's ordinary resource key, decompresses the payload,
            // parses JSON and synchronously invokes the callback only when the
            // current 1.1.6 build is below requiredVersion.
            let requested = native_required_string(&args, 0, "native_checkForcedUpdate")?;
            let callback = match args.iter().nth(1) {
                Some(Value::Function(callback)) => callback.clone(),
                _ => {
                    return Err(runtime_error(
                        "bad argument #2 to 'native_checkForcedUpdate' (function expected)"
                            .to_owned(),
                    ));
                }
            };
            let bytes =
                game_lua::text_files::load_text_bytes(&check_root, &requested, true, false, true)
                    .map_err(runtime_error)?;
            // util::JSON produces its empty/null value when the optional
            // stream is absent or cannot form a document. Missing keys then
            // make isEnabled false, so retain the same quiet behavior.
            let document = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
            if should_force_update(&document) {
                callback.call::<()>(())?;
            }
            Ok(())
        })?,
    )?;
    force_update.set(
        "native_launchAppStore",
        lua.create_function(move |_, (): ()| {
            // Zero-argument adapter sub_100026808 dispatches to
            // sub_100026238, which opens the iOS App Store product whose
            // literal identifier is 875251011. Retain the request rather than
            // attempting a platform side effect during deterministic runs.
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.requested_app_store_product = Some(("875251011".to_owned(), 3));
            bridge
                .platform_action_requests
                .push(PlatformActionRequest::OpenAppStoreProduct {
                    product_id: "875251011".to_owned(),
                    product_type: 3,
                });
            Ok(())
        })?,
    )?;
    globals.set("ForceUpdate", force_update)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_numeric_version_order_does_not_pad_components() {
        assert!(version_at_least("1.1.6", "1.1.6"));
        assert!(version_at_least("1.1.6", "1.1.5"));
        assert!(!version_at_least("1.1.6", "1.1.7"));
        assert!(!version_at_least("1.1.6", "1.1.6.0"));
        assert!(version_at_least("1.1.6.0", "1.1.6"));
        assert!(version_at_least("1.1.6", "1.1.release"));
        assert!(version_at_least(" 1.+1.6beta", "1.1.6"));
    }
}
