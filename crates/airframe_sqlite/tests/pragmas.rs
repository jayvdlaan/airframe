#![cfg(feature = "driver")]

use airframe_db::connection::{DbConnection, DbPool, SqlExec};

#[test]
fn default_foreign_keys_on_and_custom_synchronous_off() {
    let db_path =
        std::env::temp_dir().join(format!("airframe_sqlite_pragmas_{}.db", std::process::id()));
    let pool = airframe_sqlite::SqlitePool::new(db_path.to_string_lossy())
        .with_pragmas(vec!["PRAGMA synchronous=OFF".to_string()]);

    let conn = pool.get().unwrap();
    conn.ping().unwrap();

    // Verify foreign_keys is ON by default
    let rows = conn.query("PRAGMA foreign_keys", &[]).unwrap();
    assert_eq!(rows.rows.len(), 1);
    // Expect a single column with 1
    match &rows.rows[0][0] {
        airframe_db::connection::SqlValue::I64(v) => assert_eq!(*v, 1),
        other => panic!("unexpected value: {:?}", other),
    }

    // Verify synchronous is OFF (0)
    let rows2 = conn.query("PRAGMA synchronous", &[]).unwrap();
    assert_eq!(rows2.rows.len(), 1);
    match &rows2.rows[0][0] {
        airframe_db::connection::SqlValue::I64(v) => assert_eq!(*v, 0),
        other => panic!("unexpected value: {:?}", other),
    }
}
