use thiserror::Error;

/// The single error type shared by the kernel and every crate built on it.
#[derive(Debug, Error)]
pub enum LumiError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("cache error: {0}")]
    Cache(String),
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("background task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("discord error: {0}")]
    Discord(#[from] serenity::Error),
    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, LumiError>;
