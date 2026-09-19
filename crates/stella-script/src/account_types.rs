//! Host representation of the original native Rovio Account controller.

/// Gregorian calendar date used by the native registration age gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationBirthday {
    pub day: u32,
    pub month: u32,
    pub year: i32,
}

/// The original Register2 form has two choices and defaults to Male.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountGender {
    #[default]
    Male,
    Female,
}

/// Original LoginUIProvider view identities, not game Lua menu names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountView {
    SignIn,
    Register1,
    Register2,
    ThanksForRegistering,
    RegistrationFailure,
    ForgotPassword,
    PasswordResetEmailSent,
    Help1,
    Help2,
    Help3,
    NoNetworkConnectivity,
    AccountNotVerified,
}

/// Native provider field-error ids passed by `SkynestLoginUI::handleLogin`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountFieldError {
    pub field: u32,
    pub message: u32,
}

/// Independent native two-second credential editing timers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountValidationField {
    Email,
    Password,
}

/// Delayed field feedback, separate from submitted-form popup state. Contains
/// no entered text. Native iOS validity callbacks are no-ops; `valid` must not
/// become an invented button-enable rule or suppress `error`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountValidationResult {
    pub id: u64,
    /// Controller view at delivery, not necessarily the view that sent it.
    pub view: AccountView,
    pub field: AccountValidationField,
    /// Host editing revision, diagnostic only. Native already-started workers
    /// are delivered in arrival order, even after another edit in this owner.
    pub generation: u64,
    pub error: Option<AccountFieldError>,
    pub valid: bool,
}

/// Safe to display or inspect: this never contains entered credentials/tokens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountUiSnapshot {
    /// A view-owner identity; stale host input must not target a newer dialog.
    pub id: u64,
    pub view: AccountView,
    /// Native state 12 displays progress without replacing the retained view.
    pub busy: bool,
    pub field_error: Option<AccountFieldError>,
}

/// Button ids recovered from `SkynestLoginUI::handleAction` (100758558).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountUiAction {
    Back,
    Cancel,
    Continue,
    Register,
    ForgotPassword,
}
