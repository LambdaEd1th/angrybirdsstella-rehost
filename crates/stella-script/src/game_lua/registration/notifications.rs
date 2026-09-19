//! Platform notification bridge registered as one adjacent native group.

use crate::*;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct ScheduledNotification {
    fire_at: Option<Instant>,
    sequence: u64,
    _message: String,
}

#[derive(Debug)]
struct NotificationState {
    enabled: bool,
    next_sequence: u64,
    scheduled: BTreeMap<String, ScheduledNotification>,
}

impl Default for NotificationState {
    fn default() -> Self {
        Self {
            enabled: true,
            next_sequence: 0,
            scheduled: BTreeMap::new(),
        }
    }
}

/// Platform-owned local-notification queue. Purple schedules against wall
/// time through `NSDate`, independently from its scaled gameplay delta.
#[derive(Clone, Debug, Default)]
pub(crate) struct NotificationRuntime {
    state: Arc<Mutex<NotificationState>>,
}

impl NotificationRuntime {
    fn add(&self, identifier: String, delay: f64, message: String) -> bool {
        let mut state = self.state.lock().expect("notification state lock poisoned");
        if !state.enabled {
            return false;
        }

        let now = Instant::now();
        let fire_at = if delay.is_infinite() && delay.is_sign_positive() {
            // `NSDate` can represent a date beyond `Instant`'s finite range.
            // Keep that notification scheduled without spuriously firing it.
            None
        } else if !delay.is_finite() || delay <= 0.0 {
            Some(now)
        } else {
            Duration::try_from_secs_f64(delay)
                .ok()
                .and_then(|delay| now.checked_add(delay))
        };
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.wrapping_add(1);
        // sub_10053D0E0 removes the previous same-name UILocalNotification
        // before scheduling its replacement.
        state.scheduled.insert(
            identifier,
            ScheduledNotification {
                fire_at,
                sequence,
                _message: message,
            },
        );
        true
    }

    fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().expect("notification state lock poisoned");
        state.enabled = enabled;
        if !enabled {
            state.scheduled.clear();
        }
    }

    fn remove(&self, identifier: &str) -> bool {
        self.state
            .lock()
            .expect("notification state lock poisoned")
            .scheduled
            .remove(identifier)
            .is_some()
    }

    fn remove_all(&self) {
        self.state
            .lock()
            .expect("notification state lock poisoned")
            .scheduled
            .clear();
    }

    fn take_due(&self) -> Vec<String> {
        let now = Instant::now();
        let mut state = self.state.lock().expect("notification state lock poisoned");
        let mut due = state
            .scheduled
            .iter()
            .filter_map(|(identifier, notification)| {
                notification
                    .fire_at
                    .filter(|fire_at| *fire_at <= now)
                    .map(|_| (notification.sequence, identifier.clone()))
            })
            .collect::<Vec<_>>();
        due.sort_unstable_by_key(|(sequence, _)| *sequence);
        for (_, identifier) in &due {
            state.scheduled.remove(identifier);
        }
        due.into_iter().map(|(_, identifier)| identifier).collect()
    }
}

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<NotificationRuntime> {
    let runtime = NotificationRuntime::default();
    let add_runtime = runtime.clone();
    globals.set(
        "addNotificationAfter",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000883B0 requires exact STRING, NUMBER, STRING slots
            // and its generated wrapper ignores trailing Lua values.
            let identifier = native_required_string(&args, 0, "addNotificationAfter")?;
            let delay = native_required_number(&args, 1, "addNotificationAfter")?;
            let message = native_required_string(&args, 2, "addNotificationAfter")?;
            Ok(add_runtime.add(identifier, f64::from(delay as f32), message))
        })?,
    )?;
    let enable_runtime = runtime.clone();
    globals.set(
        "setNotificationsEnabled",
        lua.create_function(move |_, args: MultiValue| {
            // sub_10008962C reads one exact BOOLEAN and ignores extras.
            let enabled = native_required_boolean(&args, 0, "setNotificationsEnabled")?;
            enable_runtime.set_enabled(enabled);
            Ok(())
        })?,
    )?;
    let remove_runtime = runtime.clone();
    globals.set(
        "removeNotification",
        lua.create_function(move |_, args: MultiValue| {
            // Shared sub_100088F68/sub_100088FD0 requires exact STRING slot
            // one and does not impose an exact stack arity.
            let identifier = native_required_string(&args, 0, "removeNotification")?;
            Ok(remove_runtime.remove(&identifier))
        })?,
    )?;
    let remove_all_runtime = runtime.clone();
    globals.set(
        "removeAllNotifications",
        lua.create_function(move |_, _: MultiValue| {
            remove_all_runtime.remove_all();
            Ok(())
        })?,
    )?;
    Ok(runtime)
}

/// Deliver AppController's `userInfo.eventName` to GameLua's saved callback.
/// The virtual listener reads the callback name anew for every platform event.
pub(crate) fn dispatch_callback(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    identifier: &str,
) -> LuaResult<()> {
    // `application:didReceiveLocalNotification:` skips an absent or empty
    // eventName before synchronously walking LocalNotificationsListener.
    if identifier.is_empty() {
        return Ok(());
    }
    let callback_name = render
        .lock()
        .expect("render bridge lock poisoned")
        .notification_callback
        .clone();
    let Some(callback_name) = callback_name.filter(|name| !name.is_empty()) else {
        return Ok(());
    };
    let environment = game_environment(lua)?;
    let callback: mlua::Function = environment.get(callback_name.as_str())?;
    callback.call::<()>(identifier)
}

pub(crate) fn dispatch_due_callbacks(
    lua: &Lua,
    runtime: &NotificationRuntime,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for identifier in runtime.take_due() {
        dispatch_callback(lua, render, &identifier)?;
    }
    Ok(())
}
