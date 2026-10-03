//! SkynestIdentity4's asynchronous unregistration, distinct from logout.

use super::*;

impl SkynestAccountRuntime {
    pub(super) fn begin_unregister(&self) -> LuaResult<()> {
        // 1000A79A8 -> 1000A5844 -> 10074991C schedules 100749BC4.
        // The retained success is empty; failure 1000A5950 only copies/drops
        // its message. Neither publishes Lua results nor clears account data.
        let Some(config) = self.prepared_online_config()? else {
            self.queue_local(Completion::UnregisterFinished);
            return Ok(());
        };
        let session = self.session.clone();
        let owner = session.request_owner(ProviderLevel::Level2);
        spawn_online(
            self.online_completions.clone(),
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| {
                let _ = request_unregister(&config, &session, owner);
                OnlineCompletion::UnregisterFinished
            },
        )
    }
}

fn request_unregister(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
) -> Result<(), session::SessionError> {
    session.check_request_owner(*owner)?;
    // The worker reads the same first-external projection as native logout.
    // No known provider means no HTTP, including no session acquisition.
    let Some(network) = session
        .profile()
        .and_then(|profile| profile.active_social_network)
    else {
        return Ok(());
    };
    let body = form_body(&[(
        "provider",
        friends_support::provider_name(network).to_owned(),
    )]);
    for operation in ["external/remove", "external/disconnect"] {
        // Both calls use Level2's common form executor. The worker stores but
        // never gates on the returned status/body; there is no exact-200 rule
        // or own-profile refresh here, unlike SkynestFriends' unlink worker.
        session.execute_form_for_owner(config, owner, ProviderLevel::Level2, operation, &body)?;
    }
    session.check_request_owner(*owner)
}

#[cfg(test)]
mod tests;
