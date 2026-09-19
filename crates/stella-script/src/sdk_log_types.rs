//! Native `lang::log` records consumed by Skynest's device logger.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum SdkLogLevel {
    #[default]
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

impl SdkLogLevel {
    pub(crate) fn from_config(value: &str) -> Self {
        match value {
            "ERROR" => Self::Error,
            "WARN" => Self::Warn,
            "INFO" => Self::Info,
            "DEBUG" => Self::Debug,
            _ => Self::Off,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

/// Host diagnostics only; never contains request credentials or log contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SdkLogSnapshot {
    pub threshold: SdkLogLevel,
    pub listening: bool,
    pub queued_records: usize,
    pub in_flight_batches: usize,
    pub completed_batches: u64,
    pub failed_batches: u64,
    pub cancelled_batches: u64,
    pub last_error: Option<String>,
    /// Native executeThread terminates on an unhandled upload exception. The
    /// Rust embedding exposes that terminal failure at its next frame boundary.
    pub fatal: bool,
}
