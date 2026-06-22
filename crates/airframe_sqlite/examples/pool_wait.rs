fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "driver"))]
    {
        eprintln!("This example requires the 'driver' feature. Run with: cargo run -p airframe_sqlite --features driver --example pool_wait");
        Ok(())
    }
    #[cfg(feature = "driver")]
    {
        use airframe_db::{wait_until_ready, DbPool, SqlExec};
        use std::time::Duration;

        // Build a pool targeting a temp file DB
        let db_path =
            std::env::temp_dir().join(format!("airframe_sqlite_ready_{}.db", std::process::id()));
        let pool = airframe_sqlite::SqlitePool::new(db_path.to_string_lossy());

        // Wait until ready (ping succeeds). This is trivial for local sqlite but
        // shows how to use the helper across adapters.
        wait_until_ready(&pool, 5, Duration::from_millis(50))?;

        // Do a small operation to show it's usable
        let conn = pool.get()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ready (id INTEGER PRIMARY KEY)",
            &[],
        )?;

        println!("pool_wait example finished successfully");
        Ok(())
    }
}
