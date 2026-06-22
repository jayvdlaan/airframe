fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "driver"))]
    {
        eprintln!("This example requires the 'driver' feature. Run with: cargo run -p airframe_sqlite --features driver --example exec_query");
        Ok(())
    }
    #[cfg(feature = "driver")]
    {
        use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam, SqlValue};

        // Use a temporary file-backed database so each new connection sees the same data
        let db_path =
            std::env::temp_dir().join(format!("airframe_sqlite_example_{}.db", std::process::id()));
        let pool = airframe_sqlite::SqlitePool::new(db_path.to_string_lossy());

        // Acquire a connection (cheap handle) and verify liveness
        let conn = pool.get()?;
        conn.ping()?;

        // Create a demo table
        conn.execute(
        "CREATE TABLE IF NOT EXISTS demo (id INTEGER PRIMARY KEY, name TEXT, flag INTEGER, data BLOB)",
        &[],
    )?;

        // Insert a row
        let name = "alice";
        let data = b"xyz".as_slice();
        let n = conn.execute(
            "INSERT INTO demo (id, name, flag, data) VALUES (?1, ?2, ?3, ?4)",
            &[
                SqlParam::I64(1),
                SqlParam::Str(name),
                SqlParam::Bool(true),
                SqlParam::Bytes(data),
            ],
        )?;
        assert_eq!(n, 1);

        // Query it back
        let rows = conn.query(
            "SELECT id, name, flag, data FROM demo WHERE id = ?1",
            &[SqlParam::I64(1)],
        )?;

        println!("columns: {:?}", rows.columns);
        if let Some(first) = rows.rows.first() {
            match (&first[0], &first[1], &first[2], &first[3]) {
                (
                    SqlValue::I64(id),
                    SqlValue::Str(nm),
                    SqlValue::I64(flag),
                    SqlValue::Bytes(blob),
                ) => {
                    println!(
                        "row => id={}, name={}, flag={}, data={:?}",
                        id, nm, flag, blob
                    );
                }
                other => println!("unexpected row: {:?}", other),
            }
        }

        println!("exec_query example finished successfully");
        Ok(())
    }
}
