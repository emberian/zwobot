use thiserror::Error;

pub type Result<T> = std::result::Result<T, ZwobotError>;

#[derive(Debug, Error)]
pub enum ZwobotError {
    #[error("Model loading error: {0}")]
    ModelLoad(String),

    #[error("Inference error: {0}")]
    Inference(String),

    #[error("Zulip API error: {0}")]
    ZulipApi(String),

    #[error("Configuration error: {0}")]
    Config(String),
}
