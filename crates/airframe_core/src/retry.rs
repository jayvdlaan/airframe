//! Generic retry-with-backoff primitive shared across adapter crates.
//!
//! Adapter crates (db, mysql, net, scheduler, prefab, …) each hand-rolled their
//! own attempt loop with backoff and jitter. This is the single shared
//! implementation: it is error-type agnostic — the caller supplies a
//! `retryable` predicate deciding which errors are worth another attempt.
//!
//! Jitter is sourced from the wall clock (non-cryptographic — backoff jitter is
//! not a security decision), so this adds no new dependency to the core crate.

use std::time::Duration;

/// Backoff schedule between retry attempts.
#[derive(Debug, Clone, Copy)]
pub enum Backoff {
    /// The same delay before every retry.
    Fixed(Duration),
    /// `base * (attempt + 1)` — grows linearly.
    Linear(Duration),
    /// `base * 2^attempt`, capped at `max` — grows exponentially.
    Exponential { base: Duration, max: Duration },
}

impl Backoff {
    /// The base (pre-jitter) delay for a given 0-based attempt index.
    pub fn delay(&self, attempt: u32) -> Duration {
        match *self {
            Backoff::Fixed(d) => d,
            Backoff::Linear(base) => base.saturating_mul(attempt.saturating_add(1)),
            Backoff::Exponential { base, max } => {
                let factor = 1u32.checked_shl(attempt).unwrap_or(u32::MAX);
                base.saturating_mul(factor).min(max)
            }
        }
    }
}

/// Retry policy: how many times, with what backoff, and how much jitter.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Maximum number of *retries* (so up to `max_retries + 1` total attempts).
    pub max_retries: u32,
    /// Backoff schedule.
    pub backoff: Backoff,
    /// Jitter as a fraction of the backoff delay, in `0.0..=1.0`.
    pub jitter_frac: f32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: Backoff::Linear(Duration::from_millis(50)),
            jitter_frac: 0.2,
        }
    }
}

impl RetryPolicy {
    /// Total delay (backoff + jitter) before the retry following `attempt`.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let base = self.backoff.delay(attempt);
        let frac = self.jitter_frac.clamp(0.0, 1.0);
        if frac <= 0.0 {
            return base;
        }
        base + base.mul_f64(jitter_unit() * frac as f64)
    }
}

/// A cheap, non-cryptographic value in `[0.0, 1.0)` derived from the wall clock.
fn jitter_unit() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000_000) as f64 / 1_000_000.0
}

/// Retry a fallible operation according to `policy`.
///
/// `op` receives the 0-based attempt number. On error, `retryable(&err)` decides
/// whether to sleep (backoff + jitter) and try again. Returns the last error once
/// retries are exhausted or the error is not retryable.
pub fn retry<T, E, F, R>(policy: RetryPolicy, mut op: F, retryable: R) -> Result<T, E>
where
    F: FnMut(u32) -> Result<T, E>,
    R: Fn(&E) -> bool,
{
    let mut attempt = 0u32;
    loop {
        match op(attempt) {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt >= policy.max_retries || !retryable(&e) {
                    return Err(e);
                }
                std::thread::sleep(policy.delay_for(attempt));
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn succeeds_after_transient_failures() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy {
            max_retries: 5,
            backoff: Backoff::Fixed(Duration::from_millis(0)),
            jitter_frac: 0.0,
        };
        let out: Result<u32, &str> = retry(
            policy,
            |_| {
                let n = calls.fetch_add(1, Ordering::Relaxed);
                if n < 3 {
                    Err("transient")
                } else {
                    Ok(n)
                }
            },
            |_e| true,
        );
        assert_eq!(out, Ok(3));
        assert_eq!(calls.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn stops_on_non_retryable() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy {
            max_retries: 5,
            backoff: Backoff::Fixed(Duration::from_millis(0)),
            jitter_frac: 0.0,
        };
        let out: Result<(), &str> = retry(
            policy,
            |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                Err("fatal")
            },
            |_e| false,
        );
        assert_eq!(out, Err("fatal"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn exhausts_retries() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy {
            max_retries: 2,
            backoff: Backoff::Fixed(Duration::from_millis(0)),
            jitter_frac: 0.0,
        };
        let out: Result<(), &str> = retry(
            policy,
            |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                Err("always")
            },
            |_e| true,
        );
        assert_eq!(out, Err("always"));
        // 1 initial + 2 retries = 3 attempts
        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn exponential_backoff_caps() {
        let b = Backoff::Exponential {
            base: Duration::from_millis(10),
            max: Duration::from_millis(100),
        };
        assert_eq!(b.delay(0), Duration::from_millis(10));
        assert_eq!(b.delay(1), Duration::from_millis(20));
        assert_eq!(b.delay(3), Duration::from_millis(80));
        assert_eq!(b.delay(10), Duration::from_millis(100)); // capped
    }
}
