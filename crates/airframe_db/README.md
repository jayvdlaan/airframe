# airframe_db

Short description: Small database traits (connection, pool, SQL exec, migrations) and helpers (retry/timeout) used by adapter crates.

## Overview

airframe_db defines minimal, composable traits for database access that adapter crates implement (e.g., SQLite/MySQL). It focuses on predictability over magic, providing:
- Connection and pool traits
- Optional SQL execution for relational backends
- A simple migrations interface
- Retry and timeout helpers with consistent error modeling

This is not an ORM; it is a set of small interfaces and utilities designed to be embedded by adapter crates and higher layers.

## Logical pieces

- error::AirframeDbError: database error enum with stable integer mapping (ErrorRange::Db)
- connection::DbConnection: synchronous ping()
- connection::DbPool: get() → DbConnection
- connection::DbTx: commit()/rollback() for transactional backends
- connection::SqlExec: execute()/query() for relational adapters
- connection::Migrator: simple migration runner interface
- pool::NewConnPool: construct-on-demand pool helper; wait_until_ready()
- retry::RetryPolicy + retry(): helper for transient Connection/Timeout errors
- timeout::run_with_timeout(): coarse-grained sync timeout wrapper

## Airframe module compatibility

- Capability: provides cap:db
- Feature flag: enable with `features = ["module"]`
- Requires: none (optionally consumes config if `airframe_config` is present)
- Optional requires: cap:health (if present, registers a required `db` health check that pings the pool)
- Health: performs a simple connection ping during init; migrations can be triggered before readiness

### Configuration keys (via airframe_config)
- db.driver = sqlite|mysql (default: sqlite)
- db.url = "sqlite::memory:" | "mysql://user:pass@host/db" (default depends on driver)
- db.pool.max_size = integer (optional)
- db.pool.connect_timeout_ms = integer (optional)
- db.migrations.path = string (optional)
- db.migrations.on_start = run|skip (default: skip)

### Example: wiring with AppBuilder
```rust
// Cargo.toml
// airframe_db = { path = "../airframe_db", features = ["module"] }
// airframe_config = { path = "../airframe_config", features = ["module"] }

use airframe_core::app::AppBuilder;
use airframe_config::ConfigModule;
use airframe_db::DbModule;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with(ConfigModule::new(None))
        .with(DbModule::new())
        .build();
    app.start().await?;
    app.stop().await?;
    Ok(())
}
```

## Dependencies

- Rust dependencies: see Cargo.toml
- System libraries: none (adapters may require system libs)
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_db = { path = "../airframe_db" }
```

Typically you will also depend on a concrete adapter such as `airframe_sqlite` or `airframe_mysql` that implements these traits.

## Usage

### Example 1: Mock pool and connection

```rust
use airframe_db::{DbConnection, DbPool, Result};

#[derive(Clone, Default)]
struct MockPool;
struct MockConn;
impl DbConnection for MockConn { fn ping(&self) -> Result<()> { Ok(()) } }
impl DbPool for MockPool {
    type Conn = MockConn;
    fn get(&self) -> Result<Self::Conn> { Ok(MockConn) }
}

fn main() -> Result<()> {
    let pool = MockPool::default();
    let conn = pool.get()?;
    conn.ping()?;
    Ok(())
}
```

### Example 2: Retry and timeout helpers

```rust
use std::time::Duration;
use airframe_db::{retry, RetryPolicy, run_with_timeout, AirframeDbError, Result};

fn flaky_op() -> Result<u32> {
    // pretend an intermittent connection error
    static mut N: u32 = 0;
    unsafe { N += 1; if N < 3 { return Err(AirframeDbError::Connection) } }
    Ok(42)
}

fn main() -> Result<()> {
    let policy = RetryPolicy { max_retries: 5, backoff_ms: 10 };
    let value = retry(policy, || flaky_op())?;
    let out = run_with_timeout(Duration::from_millis(50), || Ok::<_, AirframeDbError>(value))?;
    assert_eq!(out, 42);
    Ok(())
}
```

## Adapters

- airframe_sqlite: SQLite adapter implementing DbPool/DbConnection/SqlExec and simple migrations
- airframe_mysql: MySQL adapter implementing DbPool/DbConnection/SqlExec and simple migrations
- airframe_redis: Redis-backed ByteCache (from airframe_data) — often used alongside db traits for caches
- airframe_winreg: Windows Registry adapter (Windows targets)

Each adapter crate depends on `airframe_db` and implements the relevant traits, exposing builder APIs and health checks.

### Example 3: Connect with a fake driver (runnable)

Run the example that parses a URL, validates a pool config, and performs a ping via a fake driver/pool:

```
cargo run -q -p airframe_db --example fake_connect
```

## Maintenance commands

- Coverage (HTML):
  - `cargo llvm-cov -p airframe_db --html --output-path target/coverage/airframe_db-html`
- Docs:
  - `cargo doc -p airframe_db --all-features --no-deps`

## License

Licensed under the MIT License.
