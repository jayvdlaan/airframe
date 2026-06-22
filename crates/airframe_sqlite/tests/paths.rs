#![cfg(feature = "driver")]

use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam};
use std::fs;

#[test]
fn file_path_db_is_created_and_persists() {
    let db_path =
        std::env::temp_dir().join(format!("airframe_sqlite_paths_{}.db", std::process::id()));
    let db_path_str = db_path.to_string_lossy();

    // Ensure clean slate
    let _ = fs::remove_file(&db_path);

    let pool = airframe_sqlite::SqlitePool::new(db_path_str.clone());
    // Opening should create parent dirs (already exist for temp), and file is created on first real open.
    let conn = pool.get().expect("pool.get");
    conn.ping().expect("ping");

    // Do a small DDL to force write to the file
    conn.execute(
        "CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .unwrap();

    // File should now exist
    assert!(
        db_path.exists(),
        "sqlite db file should exist at {:?}",
        db_path
    );

    // Create a new connection and verify the table exists (persistence)
    let conn2 = pool.get().unwrap();
    let rows = conn2
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name = ?1",
            &[SqlParam::Str("t")],
        )
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
}

#[test]
fn memory_db_does_not_persist_across_connections() {
    let pool = airframe_sqlite::SqlitePool::memory();
    let c1 = pool.get().unwrap();
    c1.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", &[])
        .unwrap();

    // New connection should not see the table because :memory: is per-connection
    let c2 = pool.get().unwrap();
    let rows = c2
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name = 't'",
            &[],
        )
        .unwrap();
    assert_eq!(
        rows.rows.len(),
        0,
        "':memory:' should not persist tables across connections"
    );
}
