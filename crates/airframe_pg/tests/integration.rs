// Integration tests for airframe_pg against real PostgreSQL.
// Requires PostgreSQL running on localhost:5432.
//
// Start with:
//   docker compose -f configs/docker/dev/docker-compose.yml up -d postgres
//
// Run with:
//   AIRFRAME_PG_TESTS=1 cargo test -p airframe_pg --features driver -- --test-threads=1
//
// Custom database URL:
//   AIRFRAME_PG_TESTS=1 POSTGRES_URL=postgres://user:pass@host:port/db cargo test ...

#![cfg(feature = "driver")]

use airframe_db::connection::{SqlExec, SqlParam, SqlValue};
use airframe_pg::{PgPool, PgPoolOptions};

/// Returns the database URL if tests are enabled, or None to skip.
fn pg_url() -> Option<String> {
    if std::env::var("AIRFRAME_PG_TESTS").is_ok() {
        Some(std::env::var("POSTGRES_URL").unwrap_or_else(|_| {
            "postgres://afterburner:afterburner@localhost:5432/afterburner".into()
        }))
    } else {
        None
    }
}

/// Connect with default dev pool options.
async fn connect() -> PgPool {
    let url = pg_url().expect("AIRFRAME_PG_TESTS not set");
    PgPool::connect(
        &url,
        PgPoolOptions {
            min_connections: 1,
            max_connections: 5,
            connect_timeout_secs: 5,
        },
    )
    .await
    .expect("failed to connect to postgres")
}

/// Drop the test table, ignoring errors if it doesn't exist.
async fn drop_test_table(pool: &PgPool) {
    let _ = pool
        .execute("DROP TABLE IF EXISTS _airframe_pg_test", &[])
        .await;
}

/// Create the test table with columns for every SqlParam type.
async fn create_test_table(pool: &PgPool) {
    pool.execute(
        "CREATE TABLE IF NOT EXISTS _airframe_pg_test (
            id     SERIAL PRIMARY KEY,
            name   TEXT NOT NULL,
            value  BIGINT,
            data   BYTEA,
            flag   BOOLEAN,
            score  DOUBLE PRECISION
        )",
        &[],
    )
    .await
    .expect("failed to create test table");
}

// ---------------------------------------------------------------------------
// a. Connection + ping
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_connect_and_ping() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    pool.ping().await.expect("ping failed");
}

// ---------------------------------------------------------------------------
// b. DDL — create and drop table
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ddl_create_drop() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // Verify table exists by querying the information schema
    let rows = pool
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_name = $1",
            &[SqlParam::Str("_airframe_pg_test")],
        )
        .await
        .expect("info schema query failed");
    assert_eq!(rows.rows.len(), 1);

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// c. Insert + query — all SqlParam / SqlValue type round-trips
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_insert_query_all_types() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // Insert a row with every non-null type
    pool.execute(
        "INSERT INTO _airframe_pg_test (name, value, data, flag, score) VALUES ($1, $2, $3, $4, $5)",
        &[
            SqlParam::Str("hello"),
            SqlParam::I64(42),
            SqlParam::Bytes(&[1, 2, 3]),
            SqlParam::Bool(true),
            SqlParam::F64(3.14),
        ],
    )
    .await
    .expect("insert failed");

    // Insert a row with NULLs using direct SQL (no bind params for NULLs).
    //
    // BUG: SqlParam::Null binds as None::<i64> in bind_param(), which causes
    // sqlx to send a typed NULL (INT8) to PostgreSQL. When the target column
    // is BYTEA, BOOLEAN, or FLOAT8, PostgreSQL rejects the type mismatch.
    // A proper fix would need typed null variants or an untyped NULL bind.
    // For now, test NULL round-trip using literal SQL NULLs.
    pool.execute(
        "INSERT INTO _airframe_pg_test (name, value, data, flag, score) \
         VALUES ($1, NULL, NULL, NULL, NULL)",
        &[SqlParam::Str("nulls")],
    )
    .await
    .expect("insert nulls failed");

    // Query back and verify
    let rows = pool
        .query(
            "SELECT name, value, data, flag, score FROM _airframe_pg_test ORDER BY id",
            &[],
        )
        .await
        .expect("query failed");

    assert_eq!(rows.columns, vec!["name", "value", "data", "flag", "score"]);
    assert_eq!(rows.rows.len(), 2);

    // Row 0: all types populated
    let row = &rows.rows[0];
    match &row[0] {
        SqlValue::Str(s) => assert_eq!(s, "hello"),
        other => panic!("expected Str, got {:?}", other),
    }
    match &row[1] {
        SqlValue::I64(v) => assert_eq!(*v, 42),
        other => panic!("expected I64, got {:?}", other),
    }
    match &row[2] {
        SqlValue::Bytes(b) => assert_eq!(b, &[1, 2, 3]),
        other => panic!("expected Bytes, got {:?}", other),
    }
    match &row[3] {
        SqlValue::Bool(v) => assert!(*v),
        other => panic!("expected Bool(true), got {:?}", other),
    }
    match &row[4] {
        SqlValue::F64(v) => assert!((*v - 3.14).abs() < 1e-10),
        other => panic!("expected F64, got {:?}", other),
    }

    // Row 1: null columns
    let row = &rows.rows[1];
    match &row[0] {
        SqlValue::Str(s) => assert_eq!(s, "nulls"),
        other => panic!("expected Str, got {:?}", other),
    }
    assert!(
        matches!(&row[1], SqlValue::Null),
        "expected Null for value, got {:?}",
        row[1]
    );
    assert!(
        matches!(&row[2], SqlValue::Null),
        "expected Null for data, got {:?}",
        row[2]
    );
    assert!(
        matches!(&row[3], SqlValue::Null),
        "expected Null for flag, got {:?}",
        row[3]
    );
    assert!(
        matches!(&row[4], SqlValue::Null),
        "expected Null for score, got {:?}",
        row[4]
    );

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// d. Execute — affected row count
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_execute_affected_rows() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // Insert 3 rows
    for name in &["alpha", "beta", "gamma"] {
        pool.execute(
            "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
            &[SqlParam::Str(name), SqlParam::I64(10)],
        )
        .await
        .expect("insert failed");
    }

    // Update 2 of them
    let affected = pool
        .execute(
            "UPDATE _airframe_pg_test SET value = $1 WHERE name IN ($2, $3)",
            &[
                SqlParam::I64(20),
                SqlParam::Str("alpha"),
                SqlParam::Str("beta"),
            ],
        )
        .await
        .expect("update failed");
    assert_eq!(affected, 2, "expected 2 affected rows");

    // Verify the third row is unchanged
    let rows = pool
        .query(
            "SELECT value FROM _airframe_pg_test WHERE name = $1",
            &[SqlParam::Str("gamma")],
        )
        .await
        .expect("query failed");
    match &rows.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, 10),
        other => panic!("expected I64(10), got {:?}", other),
    }

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// e. Pool concurrency — multiple simultaneous connections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pool_concurrency() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;

    let p1 = pool.clone();
    let p2 = pool.clone();
    let p3 = pool.clone();

    let (r1, r2, r3) = tokio::join!(
        p1.query("SELECT 1 AS n", &[]),
        p2.query("SELECT 2 AS n", &[]),
        p3.query("SELECT 3 AS n", &[]),
    );

    let v1 = r1.expect("query 1 failed");
    let v2 = r2.expect("query 2 failed");
    let v3 = r3.expect("query 3 failed");

    match &v1.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, 1),
        other => panic!("expected I64(1), got {:?}", other),
    }
    match &v2.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, 2),
        other => panic!("expected I64(2), got {:?}", other),
    }
    match &v3.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, 3),
        other => panic!("expected I64(3), got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// f. U64 handling — cast to i64, overflow behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_u64_handling() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // U64 value that fits in i64
    let val: u64 = 999_999;
    pool.execute(
        "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
        &[SqlParam::Str("u64_ok"), SqlParam::U64(val)],
    )
    .await
    .expect("insert U64 failed");

    let rows = pool
        .query(
            "SELECT value FROM _airframe_pg_test WHERE name = $1",
            &[SqlParam::Str("u64_ok")],
        )
        .await
        .expect("query failed");
    match &rows.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, 999_999),
        other => panic!("expected I64(999999), got {:?}", other),
    }

    // U64 value at i64::MAX boundary
    let val_max: u64 = i64::MAX as u64;
    pool.execute(
        "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
        &[SqlParam::Str("u64_max_i64"), SqlParam::U64(val_max)],
    )
    .await
    .expect("insert U64 at i64::MAX failed");

    let rows = pool
        .query(
            "SELECT value FROM _airframe_pg_test WHERE name = $1",
            &[SqlParam::Str("u64_max_i64")],
        )
        .await
        .expect("query failed");
    match &rows.rows[0][0] {
        SqlValue::I64(v) => assert_eq!(*v, i64::MAX),
        other => panic!("expected I64(i64::MAX), got {:?}", other),
    }

    // U64 value > i64::MAX — the current implementation does `*v as i64` which
    // wraps/truncates. This test documents the current behavior: the value wraps
    // to a negative i64. A future improvement could return an error instead.
    let val_overflow: u64 = (i64::MAX as u64) + 1;
    let result = pool
        .execute(
            "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
            &[SqlParam::Str("u64_overflow"), SqlParam::U64(val_overflow)],
        )
        .await;
    // Document: this currently succeeds with a wrapped negative value
    // (unlike SQLite which errors via i64::try_from). If the behavior
    // is changed to error, update this test accordingly.
    if result.is_ok() {
        let rows = pool
            .query(
                "SELECT value FROM _airframe_pg_test WHERE name = $1",
                &[SqlParam::Str("u64_overflow")],
            )
            .await
            .expect("query failed");
        match &rows.rows[0][0] {
            SqlValue::I64(v) => {
                // `(i64::MAX as u64 + 1) as i64` == i64::MIN
                assert_eq!(*v, i64::MIN, "overflow wraps to i64::MIN");
            }
            other => panic!("expected I64, got {:?}", other),
        }
    }
    // If it errored, that's also acceptable — means the impl was hardened.

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// g. SqlExec trait — sync interface via block_in_place
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sql_exec_trait() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // Use the SqlExec trait (sync interface) within a blocking context
    let pool_clone = pool.clone();
    let result = tokio::task::spawn_blocking(move || {
        let exec: &dyn SqlExec = &pool_clone;
        exec.execute(
            "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
            &[SqlParam::Str("sync_test"), SqlParam::I64(77)],
        )
        .expect("sync execute failed");

        let rows = exec
            .query(
                "SELECT name, value FROM _airframe_pg_test WHERE name = $1",
                &[SqlParam::Str("sync_test")],
            )
            .expect("sync query failed");
        rows
    })
    .await
    .expect("spawn_blocking failed");

    assert_eq!(result.rows.len(), 1);
    match &result.rows[0][0] {
        SqlValue::Str(s) => assert_eq!(s, "sync_test"),
        other => panic!("expected Str, got {:?}", other),
    }
    match &result.rows[0][1] {
        SqlValue::I64(v) => assert_eq!(*v, 77),
        other => panic!("expected I64, got {:?}", other),
    }

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// h. Migrations — run afterburner init migration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migrations_run() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;

    // Clean up from any prior run — drop the sqlx migrations table and the
    // afterburner meta table so we get a clean migration.
    let _ = pool
        .execute("DROP TABLE IF EXISTS _afterburner_meta", &[])
        .await;
    let _ = pool
        .execute("DROP TABLE IF EXISTS _sqlx_migrations", &[])
        .await;

    // Run migrations from afterburner's migrations directory.
    // The path is relative to the workspace root when running `cargo test -p airframe_pg`.
    let migrations_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../afterburner/migrations"
    );

    pool.run_migrations(migrations_dir)
        .await
        .expect("migrations failed");

    // Verify the meta table was created with schema_version = 1
    let rows = pool
        .query(
            "SELECT value FROM _afterburner_meta WHERE key = $1",
            &[SqlParam::Str("schema_version")],
        )
        .await
        .expect("meta query failed");

    assert_eq!(rows.rows.len(), 1);
    match &rows.rows[0][0] {
        SqlValue::Str(v) => assert_eq!(v, "1"),
        other => panic!("expected Str('1'), got {:?}", other),
    }

    // Running migrations again should be idempotent
    pool.run_migrations(migrations_dir)
        .await
        .expect("idempotent migration failed");

    // Clean up
    let _ = pool
        .execute("DROP TABLE IF EXISTS _afterburner_meta", &[])
        .await;
    let _ = pool
        .execute("DROP TABLE IF EXISTS _sqlx_migrations", &[])
        .await;
}

// ---------------------------------------------------------------------------
// i. SqlParam::Null type mismatch — documents a known limitation
// ---------------------------------------------------------------------------

/// SqlParam::Null binds as None::<i64>. PostgreSQL rejects this when the target
/// column is not integer-compatible (e.g., BYTEA, BOOLEAN, FLOAT8).
/// This test documents the bug. When bind_param is fixed to use untyped NULLs,
/// this test should be updated to expect success.
#[tokio::test]
async fn test_null_bind_type_mismatch() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    // SqlParam::Null into a BIGINT column works fine (same type family)
    let result = pool
        .execute(
            "INSERT INTO _airframe_pg_test (name, value) VALUES ($1, $2)",
            &[SqlParam::Str("null_i64"), SqlParam::Null],
        )
        .await;
    assert!(result.is_ok(), "NULL into BIGINT should succeed");

    // SqlParam::Null into a BOOLEAN column fails (type mismatch: INT8 vs BOOL)
    let result = pool
        .execute(
            "INSERT INTO _airframe_pg_test (name, flag) VALUES ($1, $2)",
            &[SqlParam::Str("null_bool"), SqlParam::Null],
        )
        .await;
    // This documents the current bug — remove this assertion once bind_param
    // is fixed to use untyped NULLs.
    assert!(
        result.is_err(),
        "BUG: SqlParam::Null into BOOLEAN currently fails due to typed NULL bind"
    );

    drop_test_table(&pool).await;
}

// ---------------------------------------------------------------------------
// j. Empty query result
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empty_query() {
    let Some(_) = pg_url() else { return };
    let pool = connect().await;
    drop_test_table(&pool).await;
    create_test_table(&pool).await;

    let rows = pool
        .query("SELECT * FROM _airframe_pg_test WHERE 1 = 0", &[])
        .await
        .expect("empty query failed");

    assert!(rows.rows.is_empty());
    // When no rows, columns is empty (per the implementation)
    assert!(rows.columns.is_empty());

    drop_test_table(&pool).await;
}
