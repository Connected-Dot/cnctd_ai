use crate::error::{Error, Result};
use std::time::Duration;

/// Policy for automatic retry of provider calls on transient failures.
///
/// The default policy retries up to 3 times with exponential backoff (1s, 2s, 4s),
/// capped at `max_backoff`. Errors classified retryable by [`Error::is_retryable`] are
/// retried; all other errors propagate immediately.
///
/// `RateLimited { retry_after: Some(d) }` honors the explicit duration when present,
/// falling back to the policy's exponential backoff otherwise.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Maximum number of attempts including the initial call. `1` disables retries.
    pub max_attempts: u32,
    /// Backoff for the first retry. Doubles on each subsequent attempt.
    pub base_backoff: Duration,
    /// Upper bound on the per-attempt backoff.
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// A policy that disables retries — calls are made exactly once.
    pub fn disabled() -> Self {
        Self {
            max_attempts: 1,
            base_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }

    /// Compute the backoff for retry `attempt` (1-based: 1 = first retry).
    fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(20);
        let multiplier = 1u32.checked_shl(exponent).unwrap_or(u32::MAX);
        self.base_backoff.saturating_mul(multiplier).min(self.max_backoff)
    }
}

/// Run an async closure with automatic retry on transient errors.
///
/// `f` is invoked up to `policy.max_attempts` times. Between attempts, sleeps for
/// `policy.backoff_for_attempt(n)` — or, when the error is `RateLimited` with explicit
/// `retry_after`, that duration. Non-retryable errors propagate immediately.
pub async fn with_retry<F, Fut, T>(policy: &RetryPolicy, mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_err: Option<Error> = None;
    for attempt in 0..policy.max_attempts {
        match f().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let is_last = attempt + 1 >= policy.max_attempts;
                if is_last || !err.is_retryable() {
                    return Err(err);
                }
                let delay = match &err {
                    Error::RateLimited {
                        retry_after: Some(d),
                    } => *d,
                    _ => policy.backoff_for_attempt(attempt + 1),
                };
                last_err = Some(err);
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| Error::Other("retry loop exhausted with no error recorded".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn returns_ok_on_first_try() {
        let policy = RetryPolicy::default();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let result: Result<i32> = with_retry(&policy, move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            async { Ok(42) }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_on_rate_limited_then_succeeds() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
        };
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let result: Result<i32> = with_retry(&policy, move || {
            let n = calls_c.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err(Error::RateLimited { retry_after: None })
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_on_authentication_failed() {
        let policy = RetryPolicy::default();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let result: Result<i32> = with_retry(&policy, move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            async { Err(Error::AuthenticationFailed("nope".into())) }
        })
        .await;
        assert!(matches!(result, Err(Error::AuthenticationFailed(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn surfaces_last_error_after_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 2,
            base_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
        };
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let result: Result<i32> = with_retry(&policy, move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            async { Err::<i32, _>(Error::RateLimited { retry_after: None }) }
        })
        .await;
        assert!(matches!(result, Err(Error::RateLimited { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn honors_explicit_retry_after() {
        // Policy backoff would be 60s — explicit retry_after of 50ms should win.
        let policy = RetryPolicy {
            max_attempts: 2,
            base_backoff: Duration::from_secs(60),
            max_backoff: Duration::from_secs(60),
        };
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let started = std::time::Instant::now();
        let result: Result<i32> = with_retry(&policy, move || {
            let n = calls_c.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    Err(Error::RateLimited {
                        retry_after: Some(Duration::from_millis(50)),
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        let elapsed = started.elapsed();
        assert_eq!(result.unwrap(), 42);
        assert!(elapsed >= Duration::from_millis(50));
        assert!(
            elapsed < Duration::from_secs(5),
            "should not have used 60s policy backoff, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn disabled_policy_makes_one_attempt() {
        let policy = RetryPolicy::disabled();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_c = calls.clone();
        let result: Result<i32> = with_retry(&policy, move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            async { Err::<i32, _>(Error::RateLimited { retry_after: None }) }
        })
        .await;
        assert!(matches!(result, Err(Error::RateLimited { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(5),
        };
        assert_eq!(policy.backoff_for_attempt(1), Duration::from_secs(1));
        assert_eq!(policy.backoff_for_attempt(2), Duration::from_secs(2));
        assert_eq!(policy.backoff_for_attempt(3), Duration::from_secs(4));
        // Capped at max_backoff
        assert_eq!(policy.backoff_for_attempt(4), Duration::from_secs(5));
        assert_eq!(policy.backoff_for_attempt(10), Duration::from_secs(5));
    }
}
