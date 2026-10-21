use thiserror::Error;

/// Whether retrying the failed operation could succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    Retryable,
    Permanent,
}

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
    #[error("timed out: {0}")]
    Timeout(String),
    #[error("other: {0}")]
    Other(String),
}

impl LumiError {
    /// Distinguish transient dependency failures from errors a retry cannot fix.
    #[must_use]
    pub fn failure_class(&self) -> FailureClass {
        match self {
            Self::Db(err) => db_class(err),
            Self::Redis(err) => redis_class(err),
            Self::Io(err) => io_class(err.kind()),
            Self::Task(err) => {
                if err.is_panic() {
                    FailureClass::Permanent
                } else {
                    FailureClass::Retryable
                }
            }
            Self::Discord(err) => discord_class(err),
            Self::Timeout(_) => FailureClass::Retryable,
            Self::Config(_)
            | Self::Invalid(_)
            | Self::NotFound(_)
            | Self::Cache(_)
            | Self::Json(_)
            | Self::Other(_) => FailureClass::Permanent,
        }
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.failure_class() == FailureClass::Retryable
    }
}

fn io_class(kind: std::io::ErrorKind) -> FailureClass {
    use std::io::ErrorKind;
    if matches!(
        kind,
        ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionRefused
            | ErrorKind::BrokenPipe
            | ErrorKind::TimedOut
            | ErrorKind::Interrupted
            | ErrorKind::WouldBlock
    ) {
        FailureClass::Retryable
    } else {
        FailureClass::Permanent
    }
}

fn db_class(err: &sea_orm::DbErr) -> FailureClass {
    use sea_orm::DbErr;
    match err {
        DbErr::ConnectionAcquire(_) => FailureClass::Retryable,
        DbErr::Conn(runtime) | DbErr::Query(runtime) | DbErr::Exec(runtime) => sqlx_class(runtime),
        _ => FailureClass::Permanent,
    }
}

fn sqlx_class(err: &sea_orm::error::RuntimeErr) -> FailureClass {
    use sea_orm::error::RuntimeErr;
    let RuntimeErr::SqlxError(err) = err else {
        return FailureClass::Permanent;
    };
    match err.as_ref() {
        sea_orm::sqlx::Error::Io(_)
        | sea_orm::sqlx::Error::PoolTimedOut
        | sea_orm::sqlx::Error::PoolClosed
        | sea_orm::sqlx::Error::Tls(_) => FailureClass::Retryable,
        sea_orm::sqlx::Error::Database(db) => {
            // Serialization failure, deadlock and lock-not-available are safe to retry.
            if matches!(db.code().as_deref(), Some("40001" | "40P01" | "55P03")) {
                FailureClass::Retryable
            } else {
                FailureClass::Permanent
            }
        }
        _ => FailureClass::Permanent,
    }
}

fn redis_class(err: &redis::RedisError) -> FailureClass {
    if err.is_io_error()
        || err.is_timeout()
        || err.is_connection_refusal()
        || err.is_connection_dropped()
        || err.is_cluster_error()
    {
        FailureClass::Retryable
    } else {
        FailureClass::Permanent
    }
}

fn discord_class(err: &serenity::Error) -> FailureClass {
    use serenity::Error;
    match err {
        Error::Io(_) | Error::Tungstenite(_) | Error::Gateway(_) => FailureClass::Retryable,
        Error::Http(serenity::http::HttpError::Request(_)) => FailureClass::Retryable,
        Error::Http(serenity::http::HttpError::UnsuccessfulRequest(response)) => {
            let status = response.status_code.as_u16();
            if status == 429 || status == 408 || (500..600).contains(&status) {
                FailureClass::Retryable
            } else {
                FailureClass::Permanent
            }
        }
        _ => FailureClass::Permanent,
    }
}

pub type Result<T> = std::result::Result<T, LumiError>;

#[cfg(test)]
mod tests {
    use super::{FailureClass, LumiError};

    #[test]
    fn user_errors_are_permanent() {
        for err in [
            LumiError::Invalid("bad option".into()),
            LumiError::NotFound("guild".into()),
            LumiError::Config("missing token".into()),
            LumiError::Other("unknown".into()),
        ] {
            assert_eq!(err.failure_class(), FailureClass::Permanent);
        }
    }

    #[test]
    fn timeouts_and_connection_io_are_retryable() {
        assert!(LumiError::Timeout("redis".into()).is_retryable());
        assert!(LumiError::Io(std::io::Error::from(std::io::ErrorKind::TimedOut)).is_retryable());
        assert!(
            LumiError::Io(std::io::Error::from(std::io::ErrorKind::ConnectionReset)).is_retryable()
        );
        assert!(
            !LumiError::Io(std::io::Error::from(std::io::ErrorKind::InvalidData)).is_retryable()
        );
    }

    #[test]
    fn redis_transport_failures_are_retryable() {
        let timeout = redis::RedisError::from((redis::ErrorKind::IoError, "timed out"));
        assert!(LumiError::Redis(timeout).is_retryable());

        let script = redis::RedisError::from((redis::ErrorKind::ResponseError, "bad reply"));
        assert!(!LumiError::Redis(script).is_retryable());
    }

    #[test]
    fn database_pool_errors_are_retryable_but_type_errors_are_not() {
        let pool = sea_orm::DbErr::ConnectionAcquire(sea_orm::error::ConnAcquireErr::Timeout);
        assert!(LumiError::Db(pool).is_retryable());

        let ty = sea_orm::DbErr::Type("bad column".into());
        assert!(!LumiError::Db(ty).is_retryable());
    }
}
