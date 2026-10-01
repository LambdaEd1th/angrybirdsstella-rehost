//! Observe actual runtime-request completion without changing its result.
use super::*;
use stella_script::{
    SocialFriendDetails, SocialPlatformDispatcher, SocialPlatformFriends, SocialPlatformProfile,
};

pub(super) struct Observed {
    pub inner: Arc<FacebookOAuthSession>,
    pub results: Arc<Mutex<Vec<Result<(), SocialPlatformError>>>>,
}

impl Observed {
    fn watch(&self, request: SocialProfileRequest) -> SocialProfileRequest {
        let results = self.results.clone();
        let observe = move |result: Result<SocialPlatformProfile, SocialPlatformError>| {
            results
                .lock()
                .unwrap()
                .push(result.as_ref().map(|_| ()).map_err(|error| *error));
            result
        };
        match request {
            SocialProfileRequest::Ready(result) => SocialProfileRequest::Ready(observe(result)),
            SocialProfileRequest::Pending(request) => {
                SocialProfileRequest::Pending(Box::new(move || observe(request())))
            }
        }
    }
}

impl SocialPlatformProvider for Observed {
    fn set_application_dispatcher(&self, dispatcher: SocialPlatformDispatcher) {
        self.inner.set_application_dispatcher(dispatcher);
    }
    fn is_logged_in(&self) -> bool {
        self.inner.is_logged_in()
    }
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        self.inner.clone().prepare_login()
    }
    fn application_resumed(&self) {
        self.inner.application_resumed();
    }
    fn handle_open_url(&self, url: &str) -> Result<bool, SocialPlatformError> {
        self.inner.handle_open_url(url)
    }
    fn take_login_completion(&self) -> Option<Result<(), SocialPlatformError>> {
        self.inner.take_login_completion()
    }
    fn take_login_profile_request(self: Arc<Self>) -> Option<SocialProfileRequest> {
        self.inner
            .clone()
            .take_login_profile_request()
            .map(|request| self.watch(request))
    }
    fn logout(&self) -> Result<(), SocialPlatformError> {
        self.inner.logout()
    }
    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        self.inner.user_profile()
    }
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest {
        self.watch(self.inner.clone().prepare_user_profile())
    }
    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError> {
        self.inner.publish_user_profile(profile)
    }
    fn publish_completed_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<SocialPlatformProfile, SocialPlatformError> {
        self.inner.publish_completed_profile(profile)
    }
    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        self.inner.friends(details)
    }
}
