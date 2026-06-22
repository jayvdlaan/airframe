use airframe_db::{wait_until_ready, DbConnection, DbPool, Result};
use std::time::Duration;

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

fn main() -> Result<()> {
    let pool = MockPool;

    // Acquire a connection and ping
    let conn = pool.get()?;
    conn.ping()?;

    // Wait until pool is ready (pings repeatedly up to retries)
    wait_until_ready(&pool, 3, Duration::from_millis(10))?;

    println!("mock_pool: ready and ping ok");
    Ok(())
}
