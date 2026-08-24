//! Platform notification bridge registered as one adjacent native group.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let notifications = Arc::new(Mutex::new(BTreeMap::<String, (String, f64)>::new()));
    let notifications_enabled = Arc::new(Mutex::new(true));
    let add_notifications = Arc::clone(&notifications);
    let add_notifications_enabled = Arc::clone(&notifications_enabled);
    globals.set(
        "addNotificationAfter",
        lua.create_function(
            move |_, (identifier, delay, message): (String, f64, String)| {
                if !*add_notifications_enabled
                    .lock()
                    .expect("notification state lock poisoned")
                {
                    return Ok(false);
                }
                add_notifications
                    .lock()
                    .expect("notification state lock poisoned")
                    .insert(identifier, (message, f64::from(delay as f32)));
                Ok(true)
            },
        )?,
    )?;
    let set_notifications_enabled = Arc::clone(&notifications_enabled);
    let disabled_notifications = Arc::clone(&notifications);
    globals.set(
        "setNotificationsEnabled",
        lua.create_function(move |_, enabled: bool| {
            *set_notifications_enabled
                .lock()
                .expect("notification state lock poisoned") = enabled;
            if !enabled {
                disabled_notifications
                    .lock()
                    .expect("notification state lock poisoned")
                    .clear();
            }
            Ok(())
        })?,
    )?;
    let remove_notification = Arc::clone(&notifications);
    globals.set(
        "removeNotification",
        lua.create_function(move |_, identifier: String| {
            Ok(remove_notification
                .lock()
                .expect("notification state lock poisoned")
                .remove(&identifier)
                .is_some())
        })?,
    )?;
    let remove_all_notifications = Arc::clone(&notifications);
    globals.set(
        "removeAllNotifications",
        lua.create_function(move |_, _: MultiValue| {
            remove_all_notifications
                .lock()
                .expect("notification state lock poisoned")
                .clear();
            Ok(())
        })?,
    )
}
