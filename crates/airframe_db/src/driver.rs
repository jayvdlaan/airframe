use crate::config::{parse_connection_string, ConnectionString, PoolConfig};
use crate::connection::DbPool;
use crate::Result;

/// A database driver capable of creating a pool from a parsed connection string
/// and pool configuration.
pub trait Driver {
    type Pool: DbPool;
    fn connect(&self, conn: &ConnectionString, pool: PoolConfig) -> Result<Self::Pool>;
}

/// Convenience helper: parse URL, validate PoolConfig, then delegate to the driver.
pub fn connect<D: Driver>(driver: &D, url: &str, pool: PoolConfig) -> Result<<D as Driver>::Pool> {
    let cs = parse_connection_string(url)?;
    let pool_cfg = pool.validated()?;
    driver.connect(&cs, pool_cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::DbConnection;

    #[derive(Clone, Default)]
    struct FakeConn;
    impl DbConnection for FakeConn {
        fn ping(&self) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Default, Debug)]
    struct FakePool;
    impl DbPool for FakePool {
        type Conn = FakeConn;
        fn get(&self) -> Result<Self::Conn> {
            Ok(FakeConn)
        }
    }

    #[derive(Default)]
    struct FakeDriver {
        last_scheme: std::sync::Mutex<Option<String>>,
        last_db: std::sync::Mutex<Option<String>>,
        last_max: std::sync::Mutex<Option<u32>>,
    }

    impl Driver for FakeDriver {
        type Pool = FakePool;
        fn connect(&self, conn: &ConnectionString, pool: PoolConfig) -> Result<Self::Pool> {
            *self.last_scheme.lock().unwrap() = Some(conn.scheme.clone());
            *self.last_db.lock().unwrap() = conn.database.clone();
            *self.last_max.lock().unwrap() = pool.max_size;
            Ok(FakePool)
        }
    }

    #[test]
    fn connect_helper_parses_and_validates() {
        let d = FakeDriver::default();
        let pool = PoolConfig {
            max_size: Some(2),
            connect_timeout_ms: Some(10),
        };
        let p = super::connect(&d, "mysql://user@localhost:3306/db", pool).unwrap();
        let _conn = p.get().unwrap();
        assert_eq!(d.last_scheme.lock().unwrap().as_deref(), Some("mysql"));
        assert_eq!(d.last_db.lock().unwrap().as_deref(), Some("db"));
        assert_eq!(*d.last_max.lock().unwrap(), Some(2));
    }

    #[test]
    fn connect_helper_rejects_invalid_poolcfg() {
        let d = FakeDriver::default();
        let pool = PoolConfig {
            max_size: Some(0),
            connect_timeout_ms: None,
        };
        let err = super::connect(&d, "sqlite::memory:", pool).unwrap_err();
        match err {
            crate::AirframeDbError::InvalidState => {}
            _ => panic!("unexpected error"),
        }
    }
}
