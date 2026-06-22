#![cfg(feature = "driver")]
use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam, SqlValue};

#[test]
fn exec_and_query_roundtrip() {
    // Use a file-backed DB to ensure persistence across new connections
    let db_path =
        std::env::temp_dir().join(format!("airframe_sqlite_test_{}.db", std::process::id()));
    let pool = airframe_sqlite::SqlitePool::new(db_path.to_string_lossy());
    let conn = pool.get().expect("open");
    conn.ping().expect("ping");

    conn.execute(
        "CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT, flag INTEGER, data BLOB)",
        &[],
    )
    .expect("create");

    let name = "alice";
    let data = b"xyz".as_slice();
    let n = conn
        .execute(
            "INSERT INTO demo (id, name, flag, data) VALUES (?1, ?2, ?3, ?4)",
            &[
                SqlParam::I64(1),
                SqlParam::Str(name),
                SqlParam::Bool(true),
                SqlParam::Bytes(data),
            ],
        )
        .expect("insert");
    assert_eq!(n, 1);

    let rows = conn
        .query(
            "SELECT id, name, flag, data FROM demo WHERE id = ?1",
            &[SqlParam::I64(1)],
        )
        .expect("query");
    assert_eq!(rows.columns, vec!["id", "name", "flag", "data"]);
    assert_eq!(rows.rows.len(), 1);
    let r = &rows.rows[0];
    match (&r[0], &r[1], &r[2], &r[3]) {
        (SqlValue::I64(1), SqlValue::Str(s), SqlValue::I64(f), SqlValue::Bytes(b)) => {
            assert_eq!(s, "alice");
            assert_eq!(*f, 1); // bool stored as 1
            assert_eq!(b, b"xyz");
        }
        _ => panic!("unexpected row: {:?}", r),
    }
}
