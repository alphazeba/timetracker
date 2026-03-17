use thiserror::Error;

/// All errors that can be returned by the `time-tracker-lib` library.
#[derive(Debug, Error)]
pub enum Error {
    /// No active session exists when one was required (e.g. `stop_timer`, `add_note`).
    #[error("No timer is currently running.")]
    NoActiveTimer,

    /// More than one session with `end_time IS NULL` was found — database integrity violated.
    #[error("Database integrity error: {0}")]
    DatabaseIntegrityError(String),

    /// A database or I/O error occurred; the message is a stringified description.
    #[error("External error: {0}")]
    ExternalError(String),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::ExternalError(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::ExternalError(e.to_string())
    }
}

/// Convenience `Result` type for this library.
pub type Result<T> = std::result::Result<T, Error>;
