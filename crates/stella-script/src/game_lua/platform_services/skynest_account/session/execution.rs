//! Frozen native requests, provider acquisition and the single 401 replay.
//! See `docs/native-account-request-ownership.md` for native/host boundaries.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionAdoption {
    None,
    Initial,
    EveryRenewal,
}
use ureq::{Body, http::Response};

impl IdentitySession {
    pub(in super::super) fn execute_platform_request(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
        operation: &str,
        body: (&str, &[u8]),
        still_current: &dyn Fn() -> bool,
    ) -> Result<Response<Body>, SessionError> {
        self.execute(
            config,
            owner,
            ProviderLevel::Level2,
            SessionAdoption::Initial,
            &PreparedRequest {
                url: &config.endpoint.request_url(operation),
                body: Some(body),
                timeout: super::super::REQUEST_TIMEOUT,
                still_current: Some(still_current),
            },
        )
    }

    // 10067495C obtains the current Level2 access string, then 100672054
    // sends its direct profile/own GET. No Rovio-Sgs or common401 replay.
    pub(in super::super) fn platform_profile_access(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
    ) -> Result<String, SessionError> {
        let (tokens, published) =
            self.ensure_tokens_for_owner(config, *owner, ProviderLevel::Level2)?;
        owner.generation = published;
        Ok(tokens.access_token)
    }

    #[cfg(test)]
    pub(in super::super) fn acquire_session(
        &self,
        config: &IdentityConfig,
    ) -> Result<ProfileResponse, SessionError> {
        self.acquire_session_at_epoch(config, self.epoch())
    }

    #[cfg(test)]
    pub(in super::super) fn acquire_session_at_epoch(
        &self,
        config: &IdentityConfig,
        epoch: u64,
    ) -> Result<ProfileResponse, SessionError> {
        let mut owner = self.request_owner_at_epoch(epoch, ProviderLevel::Level2)?;
        self.acquire_session_for_owner(config, &mut owner)
    }

    pub(in super::super) fn acquire_session_for_owner(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
    ) -> Result<ProfileResponse, SessionError> {
        let (_, published) = self.ensure_tokens_for_owner(config, *owner, ProviderLevel::Level2)?;
        owner.generation = published;
        let state = self.state.lock().expect("identity session lock poisoned");
        state.check_owner(owner.epoch, owner.generation)?;
        state.profile.clone().ok_or_else(SessionError::transport)
    }

    #[cfg(test)]
    fn request_owner_at_epoch(
        &self,
        epoch: u64,
        level: ProviderLevel,
    ) -> Result<RequestOwner, SessionError> {
        let owner = self.request_owner(level);
        if owner.epoch != epoch {
            return Err(SessionError::cancelled());
        }
        Ok(owner)
    }

    #[cfg(test)]
    pub(in super::super) fn execute_form(
        &self,
        config: &IdentityConfig,
        level: ProviderLevel,
        operation: &str,
        body: &str,
    ) -> Result<Response<Body>, SessionError> {
        self.execute_form_at_epoch(config, self.epoch(), level, operation, body)
    }

    #[cfg(test)]
    pub(in super::super) fn execute_form_at_epoch(
        &self,
        config: &IdentityConfig,
        epoch: u64,
        level: ProviderLevel,
        operation: &str,
        body: &str,
    ) -> Result<Response<Body>, SessionError> {
        let mut owner = self.request_owner_at_epoch(epoch, level)?;
        self.execute_form_for_owner(config, &mut owner, level, operation, body)
    }

    pub(in super::super) fn execute_form_for_owner(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
        level: ProviderLevel,
        operation: &str,
        body: &str,
    ) -> Result<Response<Body>, SessionError> {
        self.execute(
            config,
            owner,
            level,
            SessionAdoption::Initial,
            &PreparedRequest {
                url: &config.endpoint.request_url(operation),
                body: Some(("application/x-www-form-urlencoded", body.as_bytes())),
                timeout: super::super::REQUEST_TIMEOUT,
                still_current: None,
            },
        )
    }

    #[cfg(test)]
    pub(in super::super) fn execute_get(
        &self,
        config: &IdentityConfig,
        level: ProviderLevel,
        operation: &str,
    ) -> Result<Response<Body>, SessionError> {
        self.execute_get_at_epoch(config, self.epoch(), level, operation)
    }

    #[cfg(test)]
    pub(in super::super) fn execute_get_at_epoch(
        &self,
        config: &IdentityConfig,
        epoch: u64,
        level: ProviderLevel,
        operation: &str,
    ) -> Result<Response<Body>, SessionError> {
        let mut owner = self.request_owner_at_epoch(epoch, level)?;
        self.execute_get_for_owner(config, &mut owner, level, operation)
    }

    pub(in super::super) fn execute_get_for_owner(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
        level: ProviderLevel,
        operation: &str,
    ) -> Result<Response<Body>, SessionError> {
        self.execute(
            config,
            owner,
            level,
            SessionAdoption::Initial,
            &PreparedRequest {
                url: &config.endpoint.request_url(operation),
                body: None,
                timeout: super::super::REQUEST_TIMEOUT,
                still_current: None,
            },
        )
    }

    pub(in super::super) fn execute_storage(
        &self,
        config: &IdentityConfig,
        epoch: u64,
        generation: u64,
        request: &PreparedRequest<'_>,
    ) -> Result<Response<Body>, SessionError> {
        self.execute(
            config,
            &mut RequestOwner {
                epoch,
                generation: Some(generation),
            },
            ProviderLevel::Level2,
            SessionAdoption::None,
            request,
        )
    }

    pub(in super::super) fn execute_logger(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
        request: &PreparedRequest<'_>,
    ) -> Result<Response<Body>, SessionError> {
        // Device-scoped request: retain the body and use the context's Level2
        // provider, including its normal initial acquisition/one401 replay.
        self.execute(
            config,
            owner,
            ProviderLevel::Level2,
            SessionAdoption::EveryRenewal,
            request,
        )
    }

    fn execute(
        &self,
        config: &IdentityConfig,
        owner: &mut RequestOwner,
        level: ProviderLevel,
        adoption: SessionAdoption,
        request: &PreparedRequest<'_>,
    ) -> Result<Response<Body>, SessionError> {
        // URL/body are frozen before acquisition. Only authentication headers
        // change on the one retry (10066ECF4 / 10066E33C).
        request.check_current()?;
        let (tokens, published) = self.ensure_tokens_for_owner(config, *owner, level)?;
        if adoption != SessionAdoption::None {
            owner.generation = published;
        }
        // Storage is admitted with an already-resolved user prefix/body: unlike
        // an initial identity request, it cannot migrate to a newly acquired user.
        self.check_owner(owner.epoch, owner.generation)?;
        let sent = request.send(&tokens);
        self.check_owner(owner.epoch, owner.generation)?;
        request.check_current()?;
        let mut response = sent?;
        if response.status() == 401 {
            drop(response);
            {
                let mut state = self.state.lock().expect("identity session lock poisoned");
                state.check_owner(owner.epoch, owner.generation)?;
                state.tokens_mut(level).access_token.clear();
            }
            let (renewed, published) = self.ensure_tokens_for_owner(config, *owner, level)?;
            if adoption == SessionAdoption::EveryRenewal {
                owner.generation = published;
            }
            // A legitimate renewal may publish another profile. Keep that
            // session, but do not replay this old account's action under it.
            self.check_owner(owner.epoch, owner.generation)?;
            let sent = request.send(&renewed);
            self.check_owner(owner.epoch, owner.generation)?;
            request.check_current()?;
            response = sent?;
        }
        if !response.status().is_success() {
            return Err(SessionError::http(response.status().as_u16()));
        }
        Ok(response)
    }

    pub(in super::super) fn check_owner(
        &self,
        epoch: u64,
        generation: Option<u64>,
    ) -> Result<(), SessionError> {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .check_owner(epoch, generation)
    }

    fn ensure_tokens_for_owner(
        &self,
        config: &IdentityConfig,
        owner: RequestOwner,
        level: ProviderLevel,
    ) -> Result<(Tokens, Option<u64>), SessionError> {
        if (level == ProviderLevel::Level2) != owner.generation.is_some() {
            return Err(SessionError::cancelled());
        }
        {
            let state = self.state.lock().expect("identity session lock poisoned");
            state.check_owner(owner.epoch, owner.generation)?;
            if !state.tokens(level).needs_acquire(level) {
                return Ok((state.tokens(level).clone(), owner.generation));
            }
        }
        let renewal = match level {
            ProviderLevel::Level1 => &self.level1_renewal,
            ProviderLevel::Level2 => &self.level2_renewal,
        };
        let _renewal = renewal.lock().expect("identity renewal lock poisoned");
        {
            let state = self.state.lock().expect("identity session lock poisoned");
            state.check_owner(owner.epoch, owner.generation)?;
            if !state.tokens(level).needs_acquire(level) {
                return Ok((state.tokens(level).clone(), owner.generation));
            }
        }
        let published = match level {
            ProviderLevel::Level1 => {
                self.acquire_level1(config, owner.epoch, owner.generation)?;
                None
            }
            ProviderLevel::Level2 => {
                Some(self.acquire_level2(config, owner.epoch, owner.generation)?)
            }
        };
        let state = self.state.lock().expect("identity session lock poisoned");
        state.check_owner(owner.epoch, published)?;
        Ok((state.tokens(level).clone(), published))
    }
}
