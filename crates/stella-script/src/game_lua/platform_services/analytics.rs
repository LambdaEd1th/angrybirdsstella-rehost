//! Analytics service normalization and provider-broadcast boundary.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::*;

fn normalize_event_name(name: String) -> String {
    // sub_10000907C/sub_10000915C/sub_100009654 replace literal byte 0x20,
    // not general Unicode or ASCII whitespace.
    name.replace(' ', "_")
}

fn wall_clock_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(super) fn submit(
    render: &Arc<Mutex<RenderBridge>>,
    name: String,
    parameters: BTreeMap<String, String>,
) {
    render
        .lock()
        .expect("render bridge lock poisoned")
        .analytics_events
        .push(AnalyticsEvent {
            timestamp_ms: wall_clock_millis(),
            name: normalize_event_name(name),
            parameters,
        });
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let analytics_native = lua.create_table()?;
    let constructed_at = Instant::now();

    let timer_render = Arc::clone(&render);
    analytics_native.set(
        "logTimerEvent",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "logTimerEvent")?;
            // sub_100008DF8 uses (nowMs + 500 - constructionMs) / 1000,
            // formats it with `%i`, and routes through logEventWithParam.
            let elapsed_ms = constructed_at.elapsed().as_millis();
            let seconds = elapsed_ms.saturating_add(500) / 1000;
            submit(
                &timer_render,
                name,
                BTreeMap::from([("seconds".to_owned(), seconds.to_string())]),
            );
            Ok(MultiValue::new())
        })?,
    )?;

    let event_render = Arc::clone(&render);
    analytics_native.set(
        "logEvent",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "logEvent")?;
            submit(&event_render, name, BTreeMap::new());
            Ok(MultiValue::new())
        })?,
    )?;

    let parameter_render = Arc::clone(&render);
    analytics_native.set(
        "logEventWithParam",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "logEventWithParam")?;
            let key = native_required_string(&args, 1, "logEventWithParam")?;
            let value = native_required_string(&args, 2, "logEventWithParam")?;
            submit(&parameter_render, name, BTreeMap::from([(key, value)]));
            Ok(MultiValue::new())
        })?,
    )?;

    let parameter_table = analytics_native.clone();
    analytics_native.set(
        "logEventWithParams",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "logEventWithParams")?;
            let field = native_required_string(&args, 1, "logEventWithParams")?;
            // sub_100009944 resolves the named member from this native Lua
            // object and throws unless it is a table. sub_100009654 walks
            // lua_next order but inserts only STRING/STRING pairs into a
            // std::map, which both deduplicates and sorts by key.
            let Value::Table(source) = parameter_table.get::<Value>(field)? else {
                return Err(runtime_error(
                    "Tried to get an Analytics parameter table, but the field was not a table",
                ));
            };
            let mut parameters = BTreeMap::new();
            for pair in source.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if let (Value::String(key), Value::String(value)) = (key, value) {
                    parameters.insert(key.to_string_lossy(), value.to_string_lossy());
                }
            }
            submit(&render, name, parameters);
            Ok(MultiValue::new())
        })?,
    )?;
    globals.set("Analytics", analytics_native)?;
    Ok(())
}
