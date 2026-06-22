use std::time::Duration;

use airframe_db::*;

#[derive(Clone, Default)]
struct MockPool;
struct MockConn;
impl DbConnection for MockConn {
    fn ping(&self) -> Result<()> {
        Ok(())
    }
}
impl DbPool for MockPool {
    type Conn = MockConn;
    fn get(&self) -> Result<Self::Conn> {
        Ok(MockConn)
    }
}

#[test]
fn new_conn_pool_basic() {
    let pool = NewConnPool::new(|| Ok::<_, AirframeDbError>(MockConn));
    let c = pool.get().unwrap();
    c.ping().unwrap();
}

#[test]
fn wait_until_ready_works() {
    let pool = MockPool;
    wait_until_ready(&pool, 2, Duration::from_millis(1)).unwrap();
}

#[test]
fn retry_helper_succeeds_after_transient() {
    let mut n = 0u32;
    let out = retry(
        RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(1),
            jitter_frac: 0.0,
        },
        |_| {
            n += 1;
            if n < 3 {
                return Err(AirframeDbError::Connection("temp".into()));
            }
            Ok(n)
        },
    )
    .unwrap();
    assert!(out >= 3);
}

#[test]
fn timeout_helper_times_out() {
    let res = run_with_timeout(Duration::from_millis(5), || {
        std::thread::sleep(Duration::from_millis(20));
        Ok::<_, AirframeDbError>(())
    });
    assert!(matches!(res, Err(AirframeDbError::Timeout)));
}
