use crate::error::Result;

/// A database connection. Adapters should implement this over their native connection types.
pub trait DbConnection: Send + Sync + 'static {
    /// Lightweight liveness check for the connection.
    fn ping(&self) -> Result<()>;
}

/// A simple synchronous pool abstraction. Implementations are expected to be cheap to clone.
pub trait DbPool: Clone + Send + Sync + 'static {
    type Conn: DbConnection;
    fn get(&self) -> Result<Self::Conn>;
}

/// A database transaction. Consumed on commit/rollback.
pub trait DbTx: Send + 'static {
    fn commit(self) -> Result<()>;
    fn rollback(self) -> Result<()>;
}

/// Optional SQL execution trait, intended for relational adapters like SQLite/MySQL.
pub trait SqlExec {
    /// Execute a statement; returns affected rows.
    fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<u64>;
    /// Run a query returning rows in an adapter-agnostic representation.
    fn query(&self, sql: &str, params: &[SqlParam]) -> Result<SqlRows>;
}

/// A minimal typed parameter representation for SqlExec.
#[derive(Debug, Clone)]
pub enum SqlParam<'a> {
    Null,
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Str(&'a str),
    Bytes(&'a [u8]),
}

/// A simple rowset container for SqlExec::query.
#[derive(Debug, Clone, Default)]
pub struct SqlRows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SqlValue>>, // rows[row_idx][col_idx]
}

#[derive(Debug, Clone)]
pub enum SqlValue {
    Null,
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
}

/// Migrations runner trait for relational databases.
pub trait Migrator {
    /// Returns current schema version (or 0 for empty DBs)
    fn current_version(&self, conn: &dyn DbConnection) -> Result<i64>;
    /// Migrate to target version. Implementations should be idempotent and transactional.
    fn migrate_to(&self, conn: &dyn DbConnection, target: i64) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AirframeDbError;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct MockPool {
        hits: Arc<AtomicU64>,
    }
    struct MockConn;

    impl DbConnection for MockConn {
        fn ping(&self) -> Result<()> {
            Ok(())
        }
    }
    impl DbPool for MockPool {
        type Conn = MockConn;
        fn get(&self) -> Result<Self::Conn> {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Ok(MockConn)
        }
    }

    struct MockMigrator;
    impl Migrator for MockMigrator {
        fn current_version(&self, _conn: &dyn DbConnection) -> Result<i64> {
            Ok(1)
        }
        fn migrate_to(&self, _conn: &dyn DbConnection, target: i64) -> Result<()> {
            if target < 0 {
                return Err(AirframeDbError::Migration("negative".into()));
            }
            Ok(())
        }
    }

    #[test]
    fn pool_get_and_ping() {
        let p = MockPool::default();
        let c = p.get().unwrap();
        c.ping().unwrap();
    }

    #[test]
    fn migrator_basics() {
        let p = MockPool::default();
        let c = p.get().unwrap();
        let m = MockMigrator;
        assert_eq!(m.current_version(&c).unwrap(), 1);
        m.migrate_to(&c, 2).unwrap();
        let err = m.migrate_to(&c, -1).unwrap_err();
        match err {
            AirframeDbError::Migration(_) => {}
            _ => panic!("unexpected error variant"),
        }
    }
}
