//! Host controls for the platform-owned Account UI, independent of Lua menus.

use crate::*;

impl StellaLua {
    /// Submit a native SDK/engine log record to the configured device listener.
    /// This does not alter Purple's intentionally empty Lua print bindings.
    /// True means queued, not successfully uploaded. No provider means false.
    pub fn submit_sdk_log(&self, level: SdkLogLevel, tag: &str, message: &str) -> bool {
        self.skynest_account.submit_sdk_log(level, tag, message)
    }

    pub fn sdk_log_snapshot(&self) -> SdkLogSnapshot {
        self.skynest_account.sdk_log_snapshot()
    }
    /// The active native account view, if shown. A desktop presentation layer
    /// must display this above the game and route its controls through here.
    pub fn account_ui(&self) -> Option<AccountUiSnapshot> {
        self.skynest_account.ui_snapshot()
    }

    pub fn account_ui_action(&self, id: u64, action: AccountUiAction) -> Result<bool, ScriptError> {
        Ok(self.skynest_account.ui_action(id, action)?)
    }

    /// Fire a native delayed editing check. The host owns the two-second
    /// timer and skips empty text. Password validation never uses the network.
    /// Text is truncated at its first NUL, like the native NSString/strlen cache.
    pub fn validate_account_field(
        &self,
        id: u64,
        field: AccountValidationField,
        generation: u64,
        text: &str,
    ) -> Result<bool, ScriptError> {
        Ok(self
            .skynest_account
            .validate_field(id, field, generation, text)?)
    }

    /// Notify that the host invalidated/replaced a not-yet-fired editing timer.
    /// This does not cancel already-started native workers in the same owner.
    pub fn invalidate_account_validation(
        &self,
        id: u64,
        field: AccountValidationField,
        generation: u64,
    ) -> Result<bool, ScriptError> {
        Ok(self
            .skynest_account
            .invalidate_validation(id, field, generation))
    }

    pub fn take_account_validation_results(&self) -> Vec<AccountValidationResult> {
        self.skynest_account.take_validation_results()
    }

    /// Submit the native SignIn form only to the explicitly configured
    /// compatible identity provider. Credentials live only in the request,
    /// never in the snapshot, game save, diagnostic log, or script globals.
    pub fn submit_account_login(
        &self,
        id: u64,
        email: &str,
        password: &str,
    ) -> Result<bool, ScriptError> {
        Ok(self.skynest_account.submit_login(id, email, password)?)
    }

    /// Request a password-reset email from the explicitly configured identity
    /// provider. A sent-email confirmation is not an account-login success.
    pub fn submit_account_password_reset(&self, id: u64, email: &str) -> Result<bool, ScriptError> {
        Ok(self.skynest_account.submit_password_reset(id, email)?)
    }

    /// Current local calendar date, independent of mutable game Lua globals.
    pub fn account_calendar_today(&self) -> Result<RegistrationBirthday, ScriptError> {
        Ok(self.skynest_account.calendar_today()?)
    }

    /// Validate and retain Register1's birthday before showing Register2.
    pub fn submit_account_birthday(
        &self,
        id: u64,
        birthday: RegistrationBirthday,
    ) -> Result<bool, ScriptError> {
        Ok(self.skynest_account.submit_birthday(id, birthday)?)
    }

    /// Submit Register2 using this owner's validated birthday. The returned
    /// confirmation page is not login success until the user confirms it.
    pub fn submit_account_registration(
        &self,
        id: u64,
        email: &str,
        password: &str,
        gender: AccountGender,
    ) -> Result<bool, ScriptError> {
        Ok(self
            .skynest_account
            .submit_registration(id, email, password, gender)?)
    }
}
