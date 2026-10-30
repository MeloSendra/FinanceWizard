use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A rule was broken by the input; the message is shown to the user.
    #[error("{0}")]
    Invalid(String),
    #[error("Not found")]
    NotFound,
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;

pub fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(AppError::Invalid(message.into()))
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
