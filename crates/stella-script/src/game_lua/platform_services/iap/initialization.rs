//! IapManager's session listener, asynchronous result and retained retry timer.

use super::*;
use std::sync::MutexGuard;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProviderMode {
    /// Portable Telepod extension; no catalog and no store transactions.
    OfflineVouchers,
    /// Explicitly enabled zero-price host store.
    LocalStore,
    /// A compatible identity is not a platform payment provider.
    Session,
}

impl IapState {
    pub(super) fn queue(&mut self, completion: Completion) {
        self.completions.push_back((self.generation, completion));
    }

    fn replace_lifecycle(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.initialization = 0;
        self.identity = None;
        self.retry_delay = 10.0;
        self.wallet_processing = false;
        self.pending_vouchers.clear();
        // Do not clear the parallel FIFO: each old application token must
        // consume exactly its own old payload, never a replacement's result.
    }
}

impl IapRuntime {
    pub(super) fn lock_state(&self) -> MutexGuard<'_, IapState> {
        let mut state = self.state.lock().expect("IAP state lock poisoned");
        if state
            .identity
            .as_ref()
            .is_some_and(|identity| !identity.is_current())
        {
            // Host lifetime protection, not a new native wire field: retired
            // account callbacks and timers cannot initialize a replacement.
            state.replace_lifecycle();
        }
        state
    }

    pub(crate) fn enable_local_provider(&self) {
        let mut state = self.lock_state();
        if state.provider != ProviderMode::LocalStore {
            // Offline/local share the same voucher provider lifecycle. A
            // switch from an account provider retires its retained work.
            if state.provider == ProviderMode::Session {
                state.replace_lifecycle();
            }
            state.provider = ProviderMode::LocalStore;
        }
    }

    pub(crate) fn use_session_provider(&self) {
        let mut state = self.lock_state();
        if state.provider == ProviderMode::OfflineVouchers {
            state.replace_lifecycle();
            state.provider = ProviderMode::Session;
        }
    }

    pub(crate) fn session_succeeded(&self, lua: &Lua, identity: IdentityLifetime) -> LuaResult<()> {
        if !identity.is_current() {
            return Ok(());
        }
        {
            let mut state = self.lock_state();
            if state.provider == ProviderMode::Session {
                state.identity = Some(identity);
            }
        }
        complete_initialization(lua, self)?;
        Ok(())
    }

    fn begin_initialization(&self) -> bool {
        let mut state = self.lock_state();
        if state.initialization != 0
            || (state.provider == ProviderMode::Session && state.identity.is_none())
        {
            return false;
        }
        state.initialization = 1; // 0x1000CE2CC, before invoking provider.
        let succeeded = state.provider != ProviderMode::Session;
        // PaymentImpl 0x1006A8A5C / 0x1006AB9E0 posts error -2 when the
        // platform provider is absent. It is asynchronous even in this case;
        // no account HTTP 200 or config map can manufacture payment success.
        state.queue(Completion::Initialization { succeeded });
        self.application_events.post(ApplicationEvent::Iap);
        true
    }

    pub(super) fn generation_is_current(&self, generation: u64) -> bool {
        self.lock_state().generation == generation
    }

    pub(super) fn finish_initialization(&self, generation: u64) {
        let mut state = self.lock_state();
        if state.generation == generation {
            state.initialization = 2;
        }
        // 0x1000CE5C4 does not reset the exponential retry delay or cancel
        // any timer already posted by an earlier failed attempt.
    }

    pub(super) fn fail_initialization(&self, generation: u64) {
        let mut state = self.lock_state();
        if state.generation != generation {
            return;
        }
        state.initialization = 0; // 0x1000CE8B8
        // 0x1000CEA7C posts C100B8 using the current +156 float, then
        // doubles it with fminf(..., 900). The constructor starts at 10.
        self.application_events.post_delayed(
            ApplicationEvent::IapInitializationRetry(generation),
            state.retry_delay,
        );
        state.retry_delay = (state.retry_delay + state.retry_delay).min(900.0);
    }

    pub(crate) fn retry_initialization(&self, lua: &Lua, generation: u64) -> LuaResult<()> {
        if self.generation_is_current(generation) {
            complete_initialization(lua, self)?;
        }
        Ok(())
    }
}

pub(crate) fn complete_initialization(lua: &Lua, runtime: &IapRuntime) -> LuaResult<bool> {
    let environment = game_environment(lua)?;
    let Value::Function(register_callbacks) =
        environment.get::<Value>("registerPaymentCallbacks")?
    else {
        return Ok(false);
    };
    if !runtime.begin_initialization() {
        return Ok(false);
    }
    // 0x1000CE3F0 follows the provider call, but precedes its queued result.
    // The native catch selector handles rcs::CloudServiceException, not
    // lua::LuaException (LSDA 0x1008C6420). Do not swallow script failures.
    register_callbacks.call::<()>(())?;
    Ok(true)
}

#[cfg(test)]
mod tests;
