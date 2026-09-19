//! Storage ownership is a host lifetime safeguard, not an HTTP wire field.
use super::*;

impl SkynestStorageRuntime {
    pub(super) fn capture_owner(&self) -> RequestOwner {
        let sequence = self.next_sequence.get();
        self.next_sequence.set(sequence.wrapping_add(1).max(1));
        RequestOwner {
            identity: self.account.identity_lifetime(),
            storage_generation: self.generation.clone(),
            generation: self.generation.load(Ordering::Acquire),
            sequence,
        }
    }

    pub(super) fn synchronize_owner(&self) -> LuaResult<()> {
        let invalid = self
            .observed_owner
            .borrow()
            .as_ref()
            .is_none_or(|owner| !owner.is_current());
        if invalid {
            *self.online_cache.borrow_mut() = OnlineCache::default();
            self.transaction_owner.set(None);
            self.state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .transaction_in_progress = false;
            *self.observed_owner.borrow_mut() = Some(self.capture_owner());
        }
        Ok(())
    }

    pub(super) fn start_transaction(&self, owner: &RequestOwner, online: bool) -> LuaResult<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        if (!state.local_provider && !online) || !state.logged_in || state.transaction_in_progress {
            return Ok(false);
        }
        self.transaction_owner.set(Some(owner.sequence));
        state.transaction_in_progress = true;
        Ok(true)
    }

    pub(super) fn finish_transaction(&self, owner: &RequestOwner) -> LuaResult<()> {
        if self.transaction_owner.get() == Some(owner.sequence) {
            self.transaction_owner.set(None);
            self.state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .transaction_in_progress = false;
        }
        Ok(())
    }

    pub(crate) fn discard_local_completion(&self) {
        if let Some(Queued { owner, .. }) = self.pop_pending() {
            let _ = self.finish_transaction(&owner);
        }
    }

    pub(crate) fn discard_online_completion(&self) {
        if let Some(Queued { owner, completion }) = self.pop_online_pending() {
            let _ = self.finish_transaction(&owner);
            if let Some(request_id) = completion.request_id() {
                let _ = self.take_callback(request_id);
            }
        }
    }
}

impl OnlineCompletion {
    pub(super) fn request_id(&self) -> Option<u64> {
        match self {
            Self::SetKey { request_id, .. }
            | Self::SetKeyConflict { request_id, .. }
            | Self::GetKey { request_id, .. }
            | Self::GetKeyForAccountIds { request_id, .. } => Some(*request_id),
            Self::LoadCloudSettings(_)
            | Self::SaveCloudSettings { .. }
            | Self::SaveCloudSettingsConflict(_) => None,
        }
    }
}

pub(super) fn discard_local_callback(lua: &Lua, completion: Completion) -> LuaResult<()> {
    match completion {
        Completion::SetKey { callback, .. }
        | Completion::GetKey { callback, .. }
        | Completion::GetKeyForAccountIds { callback, .. } => lua.remove_registry_value(callback),
        Completion::LoadCloudSettings | Completion::SaveCloudSettings(_) => Ok(()),
    }
}
