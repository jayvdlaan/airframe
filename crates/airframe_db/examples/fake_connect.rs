use airframe_db::config::ConnectionString;
use airframe_db::{connect, DbConnection, DbPool, Driver, PoolConfig};

#[derive(Clone, Default, Debug)]
struct FakeConn;
impl DbConnection for FakeConn {
    fn ping(&self) -> airframe_db::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default, Debug)]
struct FakePool;
impl DbPool for FakePool {
    type Conn = FakeConn;
    fn get(&self) -> airframe_db::Result<Self::Conn> {
        Ok(FakeConn)
    }
}

#[derive(Default)]
struct FakeDriver;
impl Driver for FakeDriver {
    type Pool = FakePool;
    fn connect(
        &self,
        conn: &ConnectionString,
        pool: PoolConfig,
    ) -> airframe_db::Result<Self::Pool> {
        println!(
            "Connecting using scheme={} db={:?} max_size={:?}",
            conn.scheme, conn.database, pool.max_size
        );
        Ok(FakePool)
    }
}

// Run with:
// cargo run -q -p airframe_db --example fake_connect
fn main() -> airframe_db::Result<()> {
    let driver = FakeDriver;
    let pool_cfg = PoolConfig {
        max_size: Some(2),
        connect_timeout_ms: Some(1000),
    };
    let pool = connect(
        &driver,
        "mysql://user:pass@localhost:3306/mydb?ssl=off",
        pool_cfg,
    )?;
    let conn = pool.get()?;
    conn.ping()?;
    println!("fake_connect: ping ok");
    Ok(())
}
