//! Calendar/server-time Lua bindings.

use crate::*;
use std::{
    collections::VecDeque,
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const INITIAL_SYNC_REGISTRY_KEY: &str = "stella.server-time.initial-sync-complete";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
enum SyncCompletion {
    Success(i64),
    Error,
}

#[derive(Clone, Copy, Debug, Default)]
struct ServerTimeState {
    local_minus_server_seconds: i64,
    error: bool,
}

/// Cross-thread state retained by Purple's `ServerTimeImpl` owner.
#[derive(Clone, Debug)]
pub(crate) struct ServerTimeRuntime {
    compatible_url: Arc<Mutex<Option<String>>>,
    state: Arc<Mutex<ServerTimeState>>,
    completions: Arc<Mutex<VecDeque<SyncCompletion>>>,
    application_events: ApplicationEventScheduler,
}

impl ServerTimeRuntime {
    fn new(application_events: ApplicationEventScheduler) -> Self {
        Self {
            compatible_url: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(ServerTimeState::default())),
            completions: Arc::new(Mutex::new(VecDeque::new())),
            application_events,
        }
    }

    pub(crate) fn set_compatible_url(&self, url: &str) -> LuaResult<()> {
        let url = url.trim();
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"));
        let Some(host) = host else {
            return Err(runtime_error(
                "server-time URL must use the http or https scheme",
            ));
        };
        if host.is_empty() || host.starts_with('/') {
            return Err(runtime_error("server-time URL is missing a host"));
        }
        *self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("server-time URL lock poisoned"))? = Some(url.to_owned());
        Ok(())
    }

    fn compatible_url(&self) -> Option<String> {
        self.compatible_url
            .lock()
            .expect("server-time URL lock poisoned")
            .clone()
    }

    fn server_epoch_seconds(&self) -> i64 {
        unix_time_seconds()
            - self
                .state
                .lock()
                .expect("server-time state lock poisoned")
                .local_minus_server_seconds
    }

    fn status(&self) -> &'static str {
        if self
            .state
            .lock()
            .expect("server-time state lock poisoned")
            .error
        {
            "STATUS_ERROR"
        } else {
            "STATUS_OK"
        }
    }

    fn push_completion(&self, completion: SyncCompletion) {
        let mut completions = self
            .completions
            .lock()
            .expect("server-time completion queue lock poisoned");
        completions.push_back(completion);
        self.application_events.post(ApplicationEvent::ServerTime);
    }

    fn pop_completion(&self) -> Option<SyncCompletion> {
        self.completions
            .lock()
            .expect("server-time completion queue lock poisoned")
            .pop_front()
    }

    pub(crate) fn discard_completion(&self) {
        let _ = self.pop_completion();
    }
}

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    application_events: ApplicationEventScheduler,
) -> LuaResult<ServerTimeRuntime> {
    let runtime = ServerTimeRuntime::new(application_events);
    lua.set_named_registry_value(INITIAL_SYNC_REGISTRY_KEY, false)?;
    globals.set(
        "getCurrentTime",
        lua.create_function(|lua, _: MultiValue| current_time_table(lua))?,
    )?;
    let server_time = lua.create_table()?;
    let utc_runtime = runtime.clone();
    server_time.set(
        "getServerTimeInUTC",
        lua.create_function(move |lua, _: MultiValue| {
            utc_time_table_from_seconds(lua, utc_runtime.server_epoch_seconds() as f64)
        })?,
    )?;
    let local_runtime = runtime.clone();
    server_time.set(
        "getServerTimeInLocalTimeZone",
        lua.create_function(move |lua, _: MultiValue| {
            time_table_from_seconds(lua, local_runtime.server_epoch_seconds() as f64)
        })?,
    )?;
    // The offline rehost starts in the native unsynchronized state: offset
    // zero and status zero. A local synchronization keeps the zero offset but
    // still publishes Purple's asynchronous completion event; island events
    // listen for that event to rebuild their active/inactive sets.
    let sync_runtime = runtime.clone();
    server_time.set(
        "synchronizeServerTime",
        lua.create_function(move |_, _: MultiValue| {
            let Some(url) = sync_runtime.compatible_url() else {
                sync_runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("server-time state lock poisoned"))?
                    .error = false;
                sync_runtime.push_completion(SyncCompletion::Success(unix_time_seconds()));
                return Ok(());
            };
            enqueue_sync(&sync_runtime, url)?;
            Ok(())
        })?,
    )?;
    let status_runtime = runtime.clone();
    server_time.set(
        "getStatus",
        lua.create_function(move |_, _: MultiValue| Ok(status_runtime.status()))?,
    )?;
    globals.set("ServerTime", server_time)?;
    globals.set(
        "addDurationToTime",
        lua.create_function(|lua, args: MultiValue| {
            let source = native_required_table(&args, 0, "addDurationToTime")?;
            let duration = native_required_number(&args, 1, "addDurationToTime")? as f32;
            add_duration_to_time_table(lua, &source, duration)
        })?,
    )?;
    globals.set(
        "getTimeDifference",
        lua.create_function(|lua, args: MultiValue| {
            let first = native_required_table(&args, 0, "getTimeDifference")?;
            let second = native_required_table(&args, 1, "getTimeDifference")?;
            let difference =
                (time_table_seconds(lua, &first)? - time_table_seconds(lua, &second)?).abs() as u32;
            let result = lua.create_table()?;
            result.set("days", (difference / 86_400) as f32)?;
            result.set("hours", (difference / 3_600 % 24) as f32)?;
            result.set("minutes", (difference / 60 % 60) as f32)?;
            result.set("seconds", (difference % 60) as f32)?;
            Ok(result)
        })?,
    )?;
    globals.set(
        "getTimeDifferenceInSeconds",
        lua.create_function(|lua, args: MultiValue| {
            let first = native_required_table(&args, 0, "getTimeDifferenceInSeconds")?;
            let second = native_required_table(&args, 1, "getTimeDifferenceInSeconds")?;
            Ok((time_table_seconds(lua, &first)? - time_table_seconds(lua, &second)?) as f32)
        })?,
    )?;
    Ok(runtime)
}

/// Complete the constructor-triggered time synchronization after the shipped
/// Lua event dispatcher has been installed.
pub(crate) fn complete_initial_sync(lua: &Lua) -> LuaResult<()> {
    if lua.named_registry_value::<bool>(INITIAL_SYNC_REGISTRY_KEY)? {
        return Ok(());
    }
    lua.set_named_registry_value(INITIAL_SYNC_REGISTRY_KEY, true)?;
    let Value::Table(server_time) = lua.globals().get::<Value>("ServerTime")? else {
        return Ok(());
    };
    let Value::Function(synchronize) = server_time.get::<Value>("synchronizeServerTime")? else {
        return Ok(());
    };
    synchronize.call(())
}

fn unix_time_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64
}

fn parse_server_epoch(body: &[u8]) -> Option<i64> {
    fn from_json(value: &serde_json::Value) -> Option<i64> {
        match value {
            serde_json::Value::Number(number) => number
                .as_i64()
                .or_else(|| number.as_u64().and_then(|value| value.try_into().ok())),
            serde_json::Value::String(value) => value.parse().ok(),
            serde_json::Value::Object(object) => ["time", "serverTime", "timestamp", "epoch"]
                .iter()
                .find_map(|name| object.get(*name).and_then(from_json)),
            _ => None,
        }
    }

    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .as_ref()
        .and_then(from_json)
        .or_else(|| std::str::from_utf8(body).ok()?.trim().parse().ok())
}

fn request_server_epoch(url: &str) -> Option<i64> {
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .new_agent();
    let mut response = agent.get(url).call().ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .ok()?;
    parse_server_epoch(&body)
}

fn enqueue_sync(runtime: &ServerTimeRuntime, url: String) -> LuaResult<()> {
    let worker_runtime = runtime.clone();
    std::thread::Builder::new()
        .name("stella-server-time".to_owned())
        .spawn(move || {
            let completion = request_server_epoch(&url)
                .map(SyncCompletion::Success)
                .unwrap_or(SyncCompletion::Error);
            {
                let mut state = worker_runtime
                    .state
                    .lock()
                    .expect("server-time state lock poisoned");
                match completion {
                    SyncCompletion::Success(server_epoch) => {
                        *state = ServerTimeState {
                            local_minus_server_seconds: unix_time_seconds() - server_epoch,
                            error: false,
                        };
                    }
                    SyncCompletion::Error => state.error = true,
                }
            }
            worker_runtime.push_completion(completion);
        })
        .map_err(|_| runtime_error("Creating thread failed"))?;
    Ok(())
}

/// Deliver the native request continuation on the application thread.
pub(crate) fn dispatch_completion(lua: &Lua, runtime: &ServerTimeRuntime) -> LuaResult<()> {
    let Some(completion) = runtime.pop_completion() else {
        return Ok(());
    };
    if matches!(completion, SyncCompletion::Success(_)) {
        notify_server_time_synchronized(lua)?;
    }
    Ok(())
}

fn notify_server_time_synchronized(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Table(event_manager) = environment.get::<Value>("eventManager")? else {
        return Ok(());
    };
    let Value::Table(events) = environment.get::<Value>("events")? else {
        return Ok(());
    };
    let Value::Function(notify) = event_manager.get::<Value>("notify")? else {
        return Ok(());
    };
    let event_id = events.get::<Value>("EID_SERVER_TIME_SYNCHRONIZED")?;
    if matches!(event_id, Value::Nil) {
        return Ok(());
    }
    let event = lua.create_table()?;
    event.set("id", event_id)?;
    notify.call::<()>((event_manager, event))
}
