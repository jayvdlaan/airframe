use std::time::Duration;

use airframe_data::key::Key;

#[test]
fn namespace_full_key_and_formatter() {
    let k = Key::new("alpha:beta").unwrap();
    let ns = "ns1";
    let full = airframe_redis::format_namespaced_key(ns, &k);
    assert_eq!(full, "ns1::alpha:beta");

    // Also ensure format is stable if namespace changes
    let ns2 = "prod";
    let full3 = airframe_redis::format_namespaced_key(ns2, &k);
    assert_eq!(full3, "prod::alpha:beta");
}

#[test]
fn retry_policy_fails_then_succeeds() {
    use airframe_redis::{retry, AirframeRedisError, RetryPolicy};
    let mut n = 0u32;
    let policy = RetryPolicy {
        max_retries: 3,
        backoff_ms: 1,
    };
    let out = retry(policy, || {
        n += 1;
        if n < 3 {
            return Err(AirframeRedisError::Redis("fail".into()));
        }
        Ok(42)
    })
    .unwrap();
    assert_eq!(out, 42);
    assert_eq!(n, 3, "should have retried until third attempt");
}

#[test]
fn timeout_wrapper_returns_timeout() {
    use airframe_redis::{run_with_timeout, AirframeRedisError};
    let d = Duration::from_millis(10);
    let err = run_with_timeout(d, || {
        std::thread::sleep(Duration::from_millis(50));
        Ok::<u32, AirframeRedisError>(1)
    })
    .unwrap_err();
    match err {
        AirframeRedisError::Timeout => {}
        other => panic!("unexpected: {other:?}"),
    }
}
