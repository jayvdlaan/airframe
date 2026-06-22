# airframe_sqlite

Short description: Minimal SQLite adapter for the `airframe_db` traits.

## Overview

A small, predictable SQLite adapter that implements the `airframe_db` traits. It emphasizes simplicity over features: a lightweight `SqliteConn` that opens real connections per operation and a cheap-to-clone `SqlitePool` handle. Suitable for small utilities or as a building block.

## Logical pieces

- SqliteConn: implements `DbConnection` and `SqlExec`
- SqlitePool: returns `SqliteConn` handles; cloning is cheap
- Value mapping: `SqlParam`/`SqlValue` conversions for SQLite types

## Airframe module compatibility

- Compatibility: No — this crate is a direct adapter and does not export an Airframe module.

## Features

- driver: enables the real SQLite integration (pulls the optional `rusqlite` dependency). Default: off.

## Dependencies

- Rust dependencies: `rusqlite` (optional, enabled by `--features driver`)
- System libraries: SQLite3 (via `rusqlite`) on most platforms unless using its `bundled` feature in your dependency tree
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_db = { path = "../airframe_db" }
airframe_sqlite = { path = "../airframe_sqlite" }
```

This crate provides:
- SqliteConn: a connection handle that implements DbConnection and SqlExec
- SqlitePool: a cheap-to-clone pool that returns SqliteConn handles

It is designed to be small and predictable, leaving advanced pooling or async to adapter-specific crates or higher layers.

## Design notes

- rusqlite::Connection is not Send/Sync. To keep DbConnection Send + Sync, SqliteConn acts as a lightweight handle that opens a fresh rusqlite::Connection internally for each operation (ping/execute/query).
- This means an in-memory database (":memory:") will not persist across calls. Prefer a file-backed path when you want multiple operations to see the same schema/data.
- PRAGMAs: by default, the adapter enables `PRAGMA foreign_keys=ON`. You can add custom PRAGMAs via `SqlitePool::with_pragmas(Vec<String>)`, e.g. `.with_pragmas(vec!["PRAGMA synchronous=OFF".into()])`.

## Quickstart

Add to your workspace members (already present in this repository). Then use as follows:

```rust
use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam, SqlValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use a temporary file-backed database so each new connection sees the same data
    let db_path = std::env::temp_dir().join(format!("airframe_sqlite_example_{}.db", std::process::id()));
    let pool = airframe_sqlite::SqlitePool::new(db_path.to_string_lossy());

    let conn = pool.get()?;
    conn.ping()?;

    // Create a table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS demo (id INTEGER PRIMARY KEY, name TEXT, flag INTEGER, data BLOB)",
        &[],
    )?;

    // Insert a row
    let name = "alice";
    let data = b"xyz".as_slice();
    let n = conn.execute(
        "INSERT INTO demo (id, name, flag, data) VALUES (?1, ?2, ?3, ?4)",
        &[SqlParam::I64(1), SqlParam::Str(name), SqlParam::Bool(true), SqlParam::Bytes(data)],
    )?;
    assert_eq!(n, 1);

    // Query back
    let rows = conn.query(
        "SELECT id, name, flag, data FROM demo WHERE id = ?1",
        &[SqlParam::I64(1)],
    )?;

    println!("columns: {:?}", rows.columns);
    if let Some(first) = rows.rows.first() {
        match (&first[0], &first[1], &first[2], &first[3]) {
            (SqlValue::I64(id), SqlValue::Str(nm), SqlValue::I64(flag), SqlValue::Bytes(blob)) => {
                println!("row => id={}, name={}, flag={}, data={:?}", id, nm, flag, blob);
            }
            other => println!("unexpected row: {:?}", other),
        }
    }

    Ok(())
}
```

## Parameter mapping

SqlParam maps to SQLite values as follows:
- Null -> NULL
- I64(i64) -> INTEGER
- U64(u64) -> INTEGER (validated to fit i64)
- F64(f64) -> REAL
- Bool(bool) -> INTEGER (1/0)
- Str(&str) -> TEXT
- Bytes(&[u8]) -> BLOB

Query results are returned as SqlRows with:
- columns: Vec<String>
- rows: Vec<Vec<SqlValue>> where SqlValue is { Null, I64, U64, F64, Bool, Str(String), Bytes(Vec<u8>) }

Note: SQLite’s dynamic typing means booleans are typically stored as 1/0 in INTEGER columns.

## Examples

Two runnable examples are included (require the `driver` feature):
- exec_query: create table, insert, query
- pool_wait: use airframe_db::wait_until_ready then run a simple statement

Run them with:

```
cargo run -p airframe_sqlite --features driver --example exec_query
cargo run -p airframe_sqlite --features driver --example pool_wait
```

## Status

- Minimal, synchronous adapter suitable for small utilities or as a building block.
- Not a full-featured pool: SqlitePool returns a handle that opens real connections on demand.
- Feature-gated: the real driver compiles only with `--features driver`.
- File paths: parent directories are created automatically; `:memory:` creates a per-connection ephemeral DB.

## License

This project is licensed under the repository license; see the top-level LICENSE file.
