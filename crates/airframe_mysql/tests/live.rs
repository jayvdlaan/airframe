#![cfg(feature = "driver")]

use airframe_db::{wait_until_ready, DbPool, SqlExec};
use std::time::Duration;

fn mysql_url() -> Option<String> {
    std::env::var("AIRFRAME_MYSQL_URL").ok()
}

#[test]
#[ignore]
fn live_ping_and_simple_exec() {
    let Some(url) = mysql_url() else {
        return;
    }; // skip if not set
    let pool = airframe_mysql::MySqlPool::new(url);
    wait_until_ready(&pool, 10, Duration::from_millis(200)).unwrap();
    let conn = pool.get().unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS t (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(50))",
        &[],
    )
    .unwrap();
    let _ = conn
        .execute("INSERT INTO t (name) VALUES ('a')", &[])
        .unwrap();
    let rows = conn.query("SELECT COUNT(*) AS cnt FROM t", &[]).unwrap();
    assert_eq!(rows.columns.len(), 1);
}
