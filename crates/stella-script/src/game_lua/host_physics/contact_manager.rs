//! `b2ContactManager::Collide` callback boundary inside `b2World::Step`.

use crate::*;

impl StellaLua {
    pub(super) fn refresh_contact_manager(&self) -> Result<Vec<ContactEvent>, ScriptError> {
        // Contact::Update invokes the listener inline before Collide follows
        // the next intrusive-list link. Dispatch one event at a time so Lua
        // mutations remain visible to every later contact in this traversal.
        let contact_keys = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .begin_contact_manager_refresh();
        let mut contact_events = Vec::new();
        for contact_key in contact_keys {
            let prepared = {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                if let Some(contact_event) = bridge.refresh_native_contact(&contact_key) {
                    let (contact_callbacks, broken_joints) = prepare_native_contact_callbacks(
                        &self.lua,
                        &mut bridge,
                        std::slice::from_ref(&contact_event),
                    )?;
                    Some((contact_event, contact_callbacks, broken_joints))
                } else {
                    None
                }
            };
            let Some((contact_event, contact_callbacks, broken_joints)) = prepared else {
                continue;
            };
            contact_events.push(contact_event);
            for name in &broken_joints {
                dispatch_and_remove_lua_joint(&self.lua, name)?;
            }
            dispatch_native_contact_callbacks(&self.lua, &self.render, contact_callbacks)?;
        }
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .finish_contact_manager_refresh();
        Ok(contact_events)
    }
}
