use std::{fmt, io};

/// Storage diagnostics contain neither token contents nor the preferences path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacebookTokenCacheError {
    Io(io::ErrorKind),
    InvalidPreferences,
    InvalidTokenInformation,
}

impl fmt::Display for FacebookTokenCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "Facebook token preferences I/O failed: {kind}"),
            Self::InvalidPreferences => {
                f.write_str("Facebook preferences are not a property-list dictionary")
            }
            Self::InvalidTokenInformation => {
                f.write_str("Facebook token metadata has an unsupported value type")
            }
        }
    }
}
impl std::error::Error for FacebookTokenCacheError {}
