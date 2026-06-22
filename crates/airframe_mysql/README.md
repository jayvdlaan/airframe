# airframe_mysql

MySQL backend adapter for the `airframe_db` traits, with an optional Airframe module that provides `cap:db`.

## Overview

`airframe_mysql` is an L4 IO adapter that implements the `airframe_db` connection
abstractions over the [`mysql`] crate. It provides:

- `MySqlConn` — a synchronous connection handle implementing `airframe_db::connection::DbConnection`
  (`ping`) and `SqlExec` (`execute`, `query`). It maps `SqlParam`/`SqlValue` to and from
  the `mysql` crate's `Value` type. `open()` retries the connection up to 3 times and, when a
  default database is configured, runs a backtick-escaped `USE \`db\`` after connecting.
- `MySqlPool` — a cheap-to-clone `airframe_db::connection::DbPool` whose `get()` yields a fresh
  `MySqlConn`. This is a logical pool: it opens a new connection per operation rather than holding
  a server-side connection pool.
- `MySqlModule` — an optional Airframe module (behind the `module` feature) that builds a pool from
  configuration, performs a readiness ping, registers the pool in the `ServiceRegistry`, and wires an
  optional health check.

Both `MySqlConn` and `MySqlPool` expose `new(url)` and `with_db(url, db)` constructors. SQL query
text is never logged; spans record a `query_hash` instead.

## Airframe module compatibility

Yes. With the `module` feature enabled, the crate exports `MySqlModule`, which implements
`airframe_core::module::Module` and **provides `cap:db`**.

- Descriptor: name `mysql`, version `0.1.0`, `provides: ["cap:db"]`.
- Platform support: `desktop_only` (server-side deployments; not supported on mobile targets).
- On `init` it loads configuration, builds a `MySqlPool`, performs a readiness probe
  (`get()` + `ping()`), and registers `Arc<MySqlPool>` as a service.
- If `cap:health` is present, it registers a `"mysql"` health check that pings the pool.

Configuration (via `airframe_config`'s `BasicConfig`, with environment overrides):

- `mysql.url` — connection URL (default `mysql://root:@localhost:3306/`); env override `MYSQL_URL`
- `mysql.database` — optional database/schema to `USE` after connect; env override `MYSQL_DATABASE`

The `ServiceRegistryMySqlExt` trait (also `module`-gated) adds a `mysql_pool()` accessor to
`ServiceRegistry`, returning `Option<Arc<MySqlPool>>`.

Without the `module` feature, the crate is a plain adapter and exports no Airframe module.

## Dependencies

- `airframe_db` (always) — the `DbConnection`, `DbPool`, `SqlExec` traits and SQL types.
- `mysql` (optional, behind `driver`) — the real MySQL client.
- `airframe_core`, `airframe_config`, `airframe_health`, `airframe_macros` (optional, behind `module`)
  — module trait, configuration, health integration, and the `module_descriptor!` macro.
- Support crates: `anyhow`, `async-trait`, `futures`, `tracing`, `thiserror`, `semver`, `toml`.

### Features

- `driver` — enables the real MySQL implementation (`MySqlConn`, `MySqlPool`) and pulls in the
  `mysql` crate. Off by default.
- `module` — enables `MySqlModule` and `ServiceRegistryMySqlExt`. Implies `driver` and pulls in
  `airframe_core`, `airframe_config`, `airframe_health`, and `airframe_macros`. Off by default.

The default feature set is empty, so a bare dependency compiles the trait surface and helpers
without the `mysql` client.

## Usage

### Direct adapter (requires `--features driver`)

```rust
use airframe_db::connection::{DbConnection, DbPool, SqlExec};
use airframe_mysql::MySqlPool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = MySqlPool::new("mysql://root:password@127.0.0.1:3306/airframe_test");

    let conn = pool.get()?;
    conn.ping()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS t (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(50))",
        &[],
    )?;
    conn.execute("INSERT INTO t (name) VALUES ('alpha')", &[])?;

    let rows = conn.query("SELECT COUNT(*) FROM t", &[])?;
    println!("rows = {:?}", rows.rows);
    Ok(())
}
```

If the URL omits a database, use `MySqlPool::with_db(url, db)` to run a `USE` after connecting.

### As an Airframe module (requires `--features module`)

```rust
use airframe_core::app::AppBuilder;
use airframe_mysql::{MySqlModule, ServiceRegistryMySqlExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(airframe_health::HealthModule::new())
        .with(MySqlModule::new())
        .start()
        .await?;

    // cap:db consumers resolve the registered pool.
    let pool = app.services.mysql_pool().expect("MySqlPool present");
    let _conn = airframe_db::connection::DbPool::get(&*pool)?;
    Ok(())
}
```

### Examples and tests

- `examples/pool_wait.rs` — block until MySQL is reachable (run with `--features driver`).
- `examples/query_api.rs`, `examples/smoke.rs` — compile-only API demonstrations.
- `tests/smoke.rs`, `tests/dsn.rs`, `tests/traits_mock.rs` — run without any feature.
- `tests/feature_on.rs` — instantiates the real pool under `--features driver`.
- `tests/live.rs` — ignored by default; needs a live MySQL instance:

```bash
export AIRFRAME_MYSQL_URL="mysql://root:password@127.0.0.1:3306/airframe_test"
cargo test -p airframe_mysql --features driver -- --ignored
```

## Status

Pre-release (`0.5.0-beta`). The adapter is synchronous and uses a logical one-connection-per-call
pool; the `driver` and `module` features are functional but exercised primarily against local MySQL.

Licensed under MIT.
