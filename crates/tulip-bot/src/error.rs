//! Error types for tulip-bot

use thiserror::Error;

/// Errors that can occur in the tulip-bot framework
#[derive(Debug, Error)]
pub enum TulipError {
    #[error("API error: {0}")]
    Api(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Command error: {0}")]
    Command(String),

    #[error("Interaction error: {0}")]
    Interaction(String),

    #[error("Missing argument: {0}")]
    MissingArgument(String),

    #[error("Invalid argument '{name}': {reason}")]
    InvalidArgument { name: String, reason: String },

    #[cfg(feature = "runtime")]
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, TulipError>;
