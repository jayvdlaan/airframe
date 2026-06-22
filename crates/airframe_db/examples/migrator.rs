use airframe_db::{AirframeDbError, DbConnection, Migrator, Result};

// A minimal connection that always pings OK
struct MockConn;
impl DbConnection for MockConn {
    fn ping(&self) -> Result<()> {
        Ok(())
    }
}

// A toy migrator that tracks a target version in-memory when called
struct ToyMigrator;
impl Migrator for ToyMigrator {
    fn current_version(&self, _conn: &dyn DbConnection) -> Result<i64> {
        Ok(1)
    }
    fn migrate_to(&self, _conn: &dyn DbConnection, target: i64) -> Result<()> {
        if target < 1 {
            return Err(AirframeDbError::Migration("target < 1".into()));
        }
        // pretend to run steps: v1 -> ... -> target
        Ok(())
    }
}

fn main() -> Result<()> {
    let conn = MockConn;
    conn.ping()?;

    let m = ToyMigrator;
    let cur = m.current_version(&conn)?;
    println!("migrator: current version = {}", cur);
    m.migrate_to(&conn, 3)?;
    println!("migrator: migrated to target version 3");
    Ok(())
}
