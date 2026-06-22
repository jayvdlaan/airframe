//! DB retry policy — a thin wrapper over the shared [`airframe_core::retry`]
//! primitive that supplies the db-specific "is this error retryable?" predicate.

use std::time::Duration;

use crate::error::{AirframeDbError, Result};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub jitter_frac: f32, // 0.0..=1.0
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(50),
            jitter_frac: 0.2,
        }
    }
}

fn is_retryable(e: &AirframeDbError) -> bool {
    matches!(
        e,
        AirframeDbError::Connection(_) | AirframeDbError::Timeout | AirframeDbError::RetryExhausted
    )
}

/// Retry a fallible operation according to policy (linear backoff + jitter).
///
/// The closure receives the current attempt number (starting at 0). Only
/// connection/timeout/retry-exhausted errors are retried; everything else
/// returns immediately. Delegates the attempt loop to [`airframe_core::retry`].
pub fn retry<T, F>(policy: RetryPolicy, op: F) -> Result<T>
where
    F: FnMut(u32) -> Result<T>,
{
    let core_policy = airframe_core::retry::RetryPolicy {
        max_retries: policy.max_retries,
        backoff: airframe_core::retry::Backoff::Linear(policy.base_delay),
        jitter_frac: policy.jitter_frac,
    };
    airframe_core::retry::retry(core_policy, op, is_retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[test]
    fn retries_then_succeeds() {
        let counter = Arc::new(AtomicU32::new(0));
        let c2 = counter.clone();
        let out = retry(
            RetryPolicy {
                max_retries: 5,
                base_delay: Duration::from_millis(1),
                jitter_frac: 0.0,
            },
            |_n| {
                let x = c2.fetch_add(1, Ordering::Relaxed);
                if x < 3 {
                    return Err(AirframeDbError::Connection("temp".into()));
                }
                Ok(x)
            },
        )
        .unwrap();
        assert!(out >= 3);
    }

    #[test]
    fn non_retryable_bubbles() {
        let res: Result<u8> = retry(RetryPolicy::default(), |_n| {
            Err(AirframeDbError::InvalidState)
        });
        assert!(matches!(res, Err(AirframeDbError::InvalidState)));
    }
}
