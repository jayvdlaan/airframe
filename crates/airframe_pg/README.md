# airframe_pg

PostgreSQL adapter for the `airframe_db` abstractions, backed by sqlx.

## Overview

`airframe_pg` provides an async PostgreSQL connection pool (`PgPool`) built on
[`sqlx`](https://crates.io/crates/sqlx) with the tokio runtime. The pool exposes
async `connect`, `ping`, `execute`, `query`, and `run_migrations` methods, and
maps between PostgreSQL types and the adapter-agnostic `SqlParam` / `SqlValue` /
`SqlRows` types defined in `airframe_db`.

`PgPool` also implements the synchronous `airframe_db::connection::SqlExec` trait
by blocking on the async runtime via `tokio::task::block_in_place`. Async callers
should prefer the inherent async methods on `PgPool` directly; the `SqlExec` impl
exists for code that needs the sync trait and is running inside a tokio runtime.

Note: `PgPool` is a relational/SQL adapter — it implements `SqlExec` but does not
implement the `Driver` or `DbPool` traits from `airframe_db`.

The real driver implementation is gated behind the `driver` feature (which pulls
in `sqlx`); without it the crate compiles to a thin scaffold (`CRATE`, `ping`,
and the `error` module).

Parameter binding uses PostgreSQL positional placeholders (`$1`, `$2`, ...).

### Type mapping

`SqlParam` binds as:

- `Null` -> typed NULL (`None::<i64>`)
- `I64(i64)` -> BIGINT
- `U64(u64)` -> BIGINT (cast to `i64`; PostgreSQL has no unsigned integer type)
- `F64(f64)` -> DOUBLE PRECISION
- `Bool(bool)` -> BOOLEAN
- `Str(&str)` -> TEXT
- `Bytes(&[u8])` -> BYTEA

Query results decode PostgreSQL column types into `SqlValue`: `BOOL` -> `Bool`;
`INT2`/`INT4`/`INT8` (and aliases) -> `I64`; `FLOAT4`/`FLOAT8` (and aliases) ->
`F64`; `BYTEA` -> `Bytes`; all other (text-like) types -> `Str`. SQL NULLs decode
to `SqlValue::Null`.

Two known limitations are documented in the integration tests:

- `SqlParam::Null` binds as a typed (`INT8`) NULL, so binding it into a
  non-integer column (e.g. `BOOLEAN`, `BYTEA`, `FLOAT8`) is rejected by
  PostgreSQL. Use a literal SQL `NULL` for those columns until untyped NULL
  binding lands.
- `SqlParam::U64` values greater than `i64::MAX` wrap to a negative `i64` rather
  than erroring.

## Airframe module compatibility

Compatible — gated behind the `module` feature (which implies `driver`).

When the `module` feature is enabled, the crate exports `PgModule`, an Airframe
`Module` that:

- Provides the capability `cap:db.pg`.
- Reports `PlatformSupport::desktop_only` (it requires a PostgreSQL server and is
  not supported on mobile targets).
- Reads its `[postgres]` configuration from `airframe_config`'s `BasicConfig` in
  the `ServiceRegistry`:
  - `postgres.url` — connection URL (default `postgres://localhost:5432`)
  - `postgres.pool_min` — minimum idle connections (default `1`)
  - `postgres.pool_max` — maximum connections (default `10`)
  - `postgres.pool_timeout_sec` — acquire timeout in seconds (default `5`)
  - The `POSTGRES_URL` environment variable overrides `postgres.url` when set.
- Connects the pool, runs a readiness ping, and registers `Arc<PgPool>` in the
  `ServiceRegistry`.
- Registers a `postgres` health check with `airframe_health` when that service is
  present in the registry.

The `module` feature also adds the `ServiceRegistryPgExt` extension trait, whose
`pg_pool()` method returns `Option<Arc<PgPool>>` from a `ServiceRegistry`.

## Dependencies

Internal (Airframe):

- `airframe_core` — module system, `ServiceRegistry`, platform support (always).
- `airframe_db` — `SqlExec`, `SqlParam`, `SqlValue`, `SqlRows`, and the
  `AirframeDbError` / `Result` types the pool returns (always).
- `airframe_config` — `BasicConfig` for module configuration (optional, `module`).
- `airframe_health` — health-check registration (optional, `module`).
- `airframe_macros` — `module_descriptor!` macro (optional, `module`).

External:

- `sqlx` (0.8, features `runtime-tokio` + `postgres`) — the PostgreSQL driver
  and pool (optional, enabled by `driver`).
- `tokio`, `async-trait`, `futures`, `thiserror`, `tracing`, `anyhow`, `semver`.

## Usage

```rust
use airframe_db::connection::{SqlParam, SqlValue};
use airframe_pg::{PgPool, PgPoolOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect a pool (requires the `driver` feature).
    let pool = PgPool::connect(
        "postgres://localhost:5432/mydb",
        PgPoolOptions {
            min_connections: 1,
            max_connections: 10,
            connect_timeout_secs: 5,
        },
    )
    .await?;

    // Liveness check.
    pool.ping().await?;

    // Execute a statement — returns the number of affected rows.
    pool.execute(
        "CREATE TABLE IF NOT EXISTS demo (id SERIAL PRIMARY KEY, name TEXT, n BIGINT)",
        &[],
    )
    .await?;

    let affected = pool
        .execute(
            "INSERT INTO demo (name, n) VALUES ($1, $2)",
            &[SqlParam::Str("alice"), SqlParam::I64(42)],
        )
        .await?;
    assert_eq!(affected, 1);

    // Query rows in the adapter-agnostic representation.
    let rows = pool
        .query(
            "SELECT name, n FROM demo WHERE name = $1",
            &[SqlParam::Str("alice")],
        )
        .await?;

    println!("columns: {:?}", rows.columns);
    if let Some(first) = rows.rows.first() {
        if let (SqlValue::Str(name), SqlValue::I64(n)) = (&first[0], &first[1]) {
            println!("row => name={name}, n={n}");
        }
    }

    Ok(())
}
```

`PgPoolOptions` also implements `Default` (`min_connections: 1`,
`max_connections: 10`, `connect_timeout_secs: 5`). Use `PgPool::run_migrations`
to run sqlx migrations from a directory path.

## Status

Pre-release (0.5.0-beta). APIs may change before the 1.0 release.

Licensed under MIT.
