# airframe_health

Short description: Health checks, readiness barrier, and AppReady signaling for Airframe apps.

## Overview

airframe_health provides a small health system for modular Airframe applications. It lets modules register required and optional checks, exposes a readiness barrier that resolves once all required checks are Healthy, and can publish an AppReady event on the EventBus when the app becomes ready.

## Logical pieces

- HealthStatus: { Healthy | Degraded(reason) | Unhealthy(reason) }
- HealthService: register named checks; take snapshots; await readiness via `ready()`
- AppReady: lifecycle event published when required checks turn Healthy (configurable once/many)
- ServiceRegistryHealthExt: `services.health()` helper to fetch HealthService
- HealthModule: Airframe module wiring (capability `cap:health`) with polling and publish-on-ready

## Airframe module compatibility

- Compatibility: Yes — provides `cap:health` via HealthModule
- Services: registers `HealthService` into the ServiceRegistry
- Events: publishes `AppReady` on the EventBus once required checks are Healthy (configurable via `with_publish_once`)

## Dependencies

- Rust dependencies: see Cargo.toml (tokio, tokio-util, futures, dashmap, async-trait, semver)
- System libraries: none
- Airframe capacities/modules: Exports `cap:health` via HealthModule

## Setup / Installation

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_health = { path = "../airframe_health" }
```

## Usage

### HTTP health probes (canonical behavior)

Note on features: if you want ready-made Axum routes for probes, enable the `adapters-axum` feature for this crate (legacy alias also works: `http`) and use the helpers described under "Axum route adapters (optional)" below. The canonical behavior (paths and mapping) is always available without features.

The canonical HTTP probe paths and status mapping live in this crate under `airframe_health::http`.
All Airframe crates and integrations should use these helpers
so behavior stays consistent.

- Paths:
  - `airframe_health::http::PATH_READINESS` = "/readyz"
  - `airframe_health::http::PATH_LIVENESS` = "/healthz"
- Mapping helpers:
  - `map_status_to_http_readiness(&HealthStatus) -> (u16, String)`
  - `map_status_to_http_liveness(&HealthStatus) -> (u16, String)`

Semantics:
- Healthy => 200
- Degraded(msg) => 200 with body "degraded: {msg}"
- Unhealthy(msg) =>
  - Readiness: 503
  - Liveness: 500 (but transitional states like "starting"/"stopping" map to 503 to reflect not-yet-live)

Minimal example (manual use without an HTTP framework):

```rust
use airframe_health::{HealthStatus};
use airframe_health::http::{PATH_READINESS, PATH_LIVENESS, map_status_to_http_readiness, map_status_to_http_liveness};

fn respond(path: &str, status: &HealthStatus) -> (u16, String) {
    match path {
        p if p == PATH_READINESS => map_status_to_http_readiness(status),
        p if p == PATH_LIVENESS => map_status_to_http_liveness(status),
        _ => (404, "Not Found".into()),
    }
}
```

Axum/airframe_http example (mounting probes in a router):

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{Router, routing::get, http::StatusCode};
use airframe_health::{HealthStatus};
use airframe_health::http::{PATH_READINESS, PATH_LIVENESS, map_status_to_http_readiness, map_status_to_http_liveness};

fn health_router(state: Arc<RwLock<HealthStatus>>) -> Router {
    let ready_state = state.clone();
    let live_state = state.clone();
    Router::new()
        .route(PATH_READINESS, get(move || {
            let s = ready_state.clone();
            async move {
                let st = s.read().await.clone();
                let (code, body) = map_status_to_http_readiness(&st);
                (StatusCode::from_u16(code).unwrap(), body)
            }
        }))
        .route(PATH_LIVENESS, get(move || {
            let s = live_state.clone();
            async move {
                let st = s.read().await.clone();
                let (code, body) = map_status_to_http_liveness(&st);
                (StatusCode::from_u16(code).unwrap(), body)
            }
        }))
}
```

### Axum route adapters (optional; feature = "adapters-axum")

Enable the `adapters-axum` feature on `airframe_health` to get ready-made Axum route helpers and module ergonomics built on `airframe_http` (legacy alias also works: `http`):

```toml
[dependencies]
airframe_health = { path = "../airframe_health", features = ["adapters-axum"] }
airframe_http   = { path = "../airframe_http", features = ["server", "module"] }
```

Quick start using `health_router` directly:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{Router, http::StatusCode, routing::get};
use airframe_health::{HealthStatus, health_router};

let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));
let router: Router = health_router(state.clone());
// mount router into your existing axum app
```

Or mount via the RouterContributor seam:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use airframe_health::{HealthStatus, HealthProbeContributor};
use airframe_http::axum_server::RouterContributor;

let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));
let contrib = HealthProbeContributor::new(state);
let app = contrib.mount(axum::Router::new());
```

Module ergonomics (ServiceRegistry):

```rust
use airframe_health::{get_or_create_health_state, register_health_probes};
use airframe_http::axum_server::get_or_create_contrib_registry;

// inside your AppBuilder wiring code, after AxumServerModule is present:
let state = get_or_create_health_state(&app.services);
register_health_probes(&app.services, state.clone());
```

The route handlers use the canonical mapping from `airframe_health::http`, ensuring behavior remains consistent.


### Example 1: Wire HealthModule and publish AppReady

```rust
use std::sync::Arc;
use std::time::Duration;
use futures::FutureExt;
use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus;
use airframe_health::{HealthModule, HealthService, HealthStatus, AppReady};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start the app with the health module
    let app = AppBuilder::new()
        .with(HealthModule::new())
        .start()
        .await?;

    // Subscribe to AppReady before registering checks (avoid missed events)
    let mut ready = app.events.subscribe::<AppReady>()?;

    // Register a required check that becomes healthy after 100ms
    let svc = app.services.get::<HealthService>().expect("health");
    let flag = Arc::new(tokio::sync::Mutex::new(false));
    let flag2 = flag.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        *flag2.lock().await = true;
    });
    svc.register_check("delayed", true, move |_cancel| {
        let flag = flag.clone();
        async move {
            if *flag.lock().await { HealthStatus::Healthy } else { HealthStatus::Degraded("warming".into()) }
        }.boxed()
    });

    // Wait for readiness signal
    let _event = ready.recv().await.transpose()?;
    println!("AppReady observed");
    Ok(())
}
```

### Example 2: Use the readiness barrier without events

```rust
use std::sync::Arc;
use std::time::Duration;
use futures::FutureExt;
use airframe_core::app::AppBuilder;
use airframe_health::{HealthModule, HealthService, HealthStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(HealthModule::new().with_publish_once(false))
        .start()
        .await?;

    let svc = app.services.get::<HealthService>().unwrap();
    // Register multiple checks
    svc.register_check("fast", true, |_c| async move { HealthStatus::Healthy }.boxed());
    svc.register_check("slow", true, |_c| async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        HealthStatus::Healthy
    }.boxed());

    // Wait until all required checks are healthy at the same time
    svc.ready().await;
    println!("ready barrier passed");
    Ok(())
}
```

## Examples and tests

- Run the basic example: `cargo run -q -p airframe_health --example health_basic`
- Render a JSON snapshot with a dummy check: `cargo run -q -p airframe_health --example dummy_json`
- Run tests: `cargo test -q -p airframe_health`

## License

Licensed under the MIT License.
