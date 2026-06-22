fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "driver"))]
    {
        eprintln!("This example requires the 'driver' feature. Run with: cargo run -p airframe_mysql --features driver --example pool_wait");
        Ok(())
    }
    #[cfg(feature = "driver")]
    {
        use airframe_db::{wait_until_ready, DbPool, SqlExec};
        use std::time::Duration;

        // Requires a running MySQL instance. You can set AIRFRAME_MYSQL_URL, else a default is used.
        let url = std::env::var("AIRFRAME_MYSQL_URL")
            .unwrap_or_else(|_| "mysql://root:password@127.0.0.1:3306/airframe_test".to_string());

        let pool = airframe_mysql::MySqlPool::new(url);

        // Wait until ready (ping succeeds)
        wait_until_ready(&pool, 20, Duration::from_millis(250))?;

        // Create table and insert row
        let conn = pool.get()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ready (id INT PRIMARY KEY AUTO_INCREMENT)",
            &[],
        )?;
        let affected = conn.execute("INSERT INTO ready () VALUES ()", &[])?;
        println!("Inserted rows: {}", affected);

        // Query back count
        let rows = conn.query("SELECT COUNT(*) AS cnt FROM ready", &[])?;
        println!("Columns: {:?}, Rows: {:?}", rows.columns, rows.rows);

        println!("pool_wait example finished successfully");
        Ok(())
    }
}
