//! Frozen common authenticated request, shared by identity and storage callers.

use super::{SessionError, Tokens};
use std::time::Duration;
use ureq::{Body, http::Response};

/// No Debug implementation: URLs/bodies may contain account-owned data.
pub(in super::super) struct PreparedRequest<'a> {
    pub(in super::super) url: &'a str,
    /// None selects GET; Some preserves POST's exact content type and bytes.
    pub(in super::super) body: Option<(&'a str, &'a [u8])>,
    pub(in super::super) timeout: Duration,
    pub(in super::super) still_current: Option<&'a dyn Fn() -> bool>,
}

impl PreparedRequest<'_> {
    pub(super) fn check_current(&self) -> Result<(), SessionError> {
        if self.still_current.is_some_and(|current| !current()) {
            return Err(SessionError::cancelled());
        }
        Ok(())
    }

    pub(super) fn send(&self, tokens: &Tokens) -> Result<Response<Body>, SessionError> {
        self.check_current()?;
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(self.timeout))
            .build()
            .new_agent();
        // 10066E978 uses the selected provider's access + segment, never the
        // clientSignature metadata. Only these headers change on first-401 replay.
        match self.body {
            Some((content_type, body)) => agent
                .post(self.url)
                .header("Content-Type", content_type)
                .header("X-Access-Token", &tokens.access_token)
                .header("Rovio-Sgs", &tokens.segment)
                .send(body),
            None => agent
                .get(self.url)
                .header("X-Access-Token", &tokens.access_token)
                .header("Rovio-Sgs", &tokens.segment)
                .call(),
        }
        .map_err(|_| SessionError::transport())
    }
}
