use std::time::Duration;

// Internal deadline math uses spacetime_core for portability, while the
// public API remains on std::time::Duration and std sync primitives.
use spacetime_core as st;

use crate::error::{AirframeDbError, Result};

/// Run a blocking operation with a timeout by spawning a thread and joining with timeout.
/// Note: This is a coarse mechanism for sync code; adapters may provide more precise timeouts.
pub fn run_with_timeout<T: Send + 'static>(
    dur: Duration,
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let handle = std::thread::spawn(f);
    match handle.join_timeout(dur) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AirframeDbError::Timeout),
    }
}

trait JoinTimeout<T> {
    fn join_timeout(self, timeout: Duration) -> std::result::Result<T, ()>;
}

impl<T: Send + 'static> JoinTimeout<T> for std::thread::JoinHandle<T> {
    fn join_timeout(self, timeout: Duration) -> std::result::Result<T, ()> {
        use std::sync::{Arc, Condvar, Mutex};
        let pair = Arc::new((Mutex::new(None), Condvar::new()));
        let pair2 = pair.clone();
        std::thread::spawn(move || {
            let res = self.join();
            let (lock, cv) = &*pair2;
            *lock.lock().unwrap() = Some(res);
            cv.notify_one();
        });
        // Compute absolute deadline using spacetime_core::Instant
        fn now_ms() -> u64 {
            let now = std::time::SystemTime::now();
            now.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_millis(0))
                .as_millis() as u64
        }
        let start = st::Instant::from_millis_since_epoch(now_ms());
        let deadline = start.saturating_add(st::Duration::from_millis(timeout.as_millis() as u64));

        let (lock, cv) = &*pair;
        let mut guard = lock.lock().unwrap();
        loop {
            if let Some(res) = guard.take() {
                break res.map_err(|_| ());
            }
            // Compute remaining time until deadline
            let now = st::Instant::from_millis_since_epoch(now_ms());
            let rem = if deadline >= now {
                deadline.saturating_duration_since(now)
            } else {
                st::Duration::zero()
            };
            if rem.millis == 0 {
                break Err(());
            }
            let std_rem = Duration::from_millis(rem.millis);
            let (g, _waitres) = cv.wait_timeout(guard, std_rem).unwrap();
            guard = g;
            // loop to re-check condition (handles spurious wakeups)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn quick_ok() {
        let out =
            run_with_timeout(Duration::from_millis(50), || Ok::<_, AirframeDbError>(42)).unwrap();
        assert_eq!(out, 42);
    }

    #[test]
    fn times_out() {
        let start = Instant::now();
        let res = run_with_timeout(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(30));
            Ok::<_, AirframeDbError>(())
        });
        assert!(matches!(res, Err(AirframeDbError::Timeout)));
        assert!(start.elapsed() >= Duration::from_millis(10));
    }
}
