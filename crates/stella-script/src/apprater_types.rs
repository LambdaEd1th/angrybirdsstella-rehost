//! Platform alert presented by the original Apprater service.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRatingChoice {
    Later,
    Decline,
    Rate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRatingButton {
    pub choice: AppRatingChoice,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRatingPrompt {
    /// Retained alert owner; stale input cannot answer a subsequent prompt.
    pub id: u64,
    /// Purple supplies an empty alert title and puts this text in its message.
    pub message: String,
    /// Native order: remind later, decline, rate.
    pub buttons: [AppRatingButton; 3],
}
