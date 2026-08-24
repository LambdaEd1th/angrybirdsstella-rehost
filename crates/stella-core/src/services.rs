#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError {
    Offline,
    Unsupported,
    Failed(String),
}

/// Replaceable boundary for services that were backed by 2014-era iOS SDKs.
pub trait PlatformServices {
    fn sign_in(&mut self) -> Result<(), ServiceError>;
    fn unlock_achievement(&mut self, id: &str) -> Result<(), ServiceError>;
    fn submit_score(&mut self, board: &str, score: i64) -> Result<(), ServiceError>;
    fn purchase(&mut self, product: &str) -> Result<(), ServiceError>;
    fn show_ad(&mut self, placement: &str) -> Result<(), ServiceError>;
}

#[derive(Debug, Default)]
pub struct NullServices;

impl PlatformServices for NullServices {
    fn sign_in(&mut self) -> Result<(), ServiceError> {
        Err(ServiceError::Offline)
    }

    fn unlock_achievement(&mut self, _id: &str) -> Result<(), ServiceError> {
        Ok(())
    }

    fn submit_score(&mut self, _board: &str, _score: i64) -> Result<(), ServiceError> {
        Ok(())
    }

    fn purchase(&mut self, _product: &str) -> Result<(), ServiceError> {
        Err(ServiceError::Unsupported)
    }

    fn show_ad(&mut self, _placement: &str) -> Result<(), ServiceError> {
        Ok(())
    }
}
