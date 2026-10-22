//! Bounded timeouts and transient-failure retries shared by outbound calls.
//!
//! Only errors classified [`crate::FailureClass::Retryable`] are retried; a
//! permanent error returns immediately so a user input error never turns into
//! a retry storm.

use std::future::Future;
use std::time::Duration;

use crate::error::{LumiError, Result};

/// Upper bound for one outbound attempt. Callers override this when they know
/// their dependency should answer faster.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Run `future` with a hard deadline, reporting the operation on expiry.
pub async fn with_timeout<T, F>(what: &'static str, limit: Duration, future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    tokio::time::timeout(limit, future)
        .await
        .map_err(|_| LumiError::Timeout(what.to_string()))?
}

/// Retry `op` at most `attempts` times in total, backing off exponentially from
/// `base` and stopping early on a permanent error.
pub async fn retry_transient<T, F, Fut>(attempts: u32, base: Duration, mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut remaining = attempts.max(1);
    let mut delay = base;

    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) if err.is_retryable() && remaining > 1 => {
                remaining -= 1;
                tracing::warn!(
                    attempts_left = remaining,
                    delay_ms = delay.as_millis(),
                    error = %err,
                    "retryable outbound failure; backing off"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(MAX_BACKOFF);
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{retry_transient, with_timeout};
    use crate::error::LumiError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn timeout_reports_a_typed_error() {
        let result: crate::Result<()> =
            with_timeout("slow dependency", Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Ok(())
            })
            .await;

        match result {
            Err(LumiError::Timeout(what)) => assert_eq!(what, "slow dependency"),
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retries_transient_errors_then_succeeds() {
        let calls = Arc::new(AtomicU32::new(0));
        let result = retry_transient(3, Duration::from_millis(1), || {
            let calls = Arc::clone(&calls);
            async move {
                if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(LumiError::Timeout("attempt".into()))
                } else {
                    Ok(7_u32)
                }
            }
        })
        .await;

        assert_eq!(result.ok(), Some(7));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn permanent_errors_are_not_retried() {
        let calls = Arc::new(AtomicU32::new(0));
        let result = retry_transient(5, Duration::from_millis(1), || {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(LumiError::Invalid("bad input".into()))
            }
        })
        .await;

        let Err(_) = result else {
            panic!("expected error");
        };
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_are_bounded() {
        let calls = Arc::new(AtomicU32::new(0));
        let result = retry_transient(2, Duration::from_millis(1), || {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(LumiError::Timeout("still down".into()))
            }
        })
        .await;

        let Err(_) = result else {
            panic!("expected error");
        };
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
