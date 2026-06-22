# airframe_http

Short description: HTTP client/server traits and integrations (Axum server, Reqwest client) for Airframe apps.

## Overview

airframe_http provides a small set of HTTP abstractions and ready-to-use integrations:
- Core API traits for HTTP clients and a spec-driven facade
- A reqwest-based client implementation (feature `client`)
- Axum server helpers and an Airframe module to run a server (feature `server` + `module`)
- A RouterContributor seam so multiple modules can add routes to one Axum router

## Logical pieces

- api::client::HttpClient, InvokeError: trait and error type for making HTTP calls
- api::spec_client::SpecClient: helper that binds an HttpClient to an API spec
- clients::reqwest::ReqwestClient: concrete HttpClient built on reqwest (feature `client`)
- server::router_contrib::{RouterContributor, RouterContribRegistry, mount_all}: compose routers from contributors (feature `server`)
- server::axum_server::{AxumServer, BoundAddr}: thin Axum server wrapper (feature `server`)
- AxumServerModule: Airframe module that starts an HTTP server and exposes BoundAddr (features `server`, `module`)
- ReqwestClientModule: Airframe module that registers an HttpClient in the ServiceRegistry (features `client`, `module`)
- AdminModule: optional module that contributes admin routes (features `server`, `module`)

## Airframe module compatibility

- Compatibility: Yes — when built with `module` feature
- Provides capabilities:
  - `cap:http.server` via AxumServerModule (features `server`, `module`)
  - `cap:http.client` via ReqwestClientModule (features `client`, `module`)
  - RouterContribRegistry is registered lazily when needed to collect RouterContributors

## Dependencies

- Rust crate features:
  - `client`: enables the reqwest client and requires Tokio
  - `server`: enables Axum server and router composition utilities; requires Tokio
  - `module`: enables Airframe module integration points (depends on airframe_core)
  - `config`: optional config integration (if used by server for graceful settings)
- Rust dependencies: see Cargo.toml (reqwest, axum, hyper, tokio when corresponding features are enabled)
- System libraries: none (pure Rust + OS networking)
- Airframe capacities/modules: `cap:http.server`, `cap:http.client` as above

## Setup / Installation

Base dependency (API traits only):

```toml
[dependencies]
airframe_http = { path = "../airframe_http" }
```

Reqwest client only:

```toml
[dependencies]
airframe_http = { path = "../airframe_http", features = ["client"] }
```

Axum server as an Airframe module:

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_http = { path = "../airframe_http", features = ["server", "module"] }
```

Client + Server together for an end-to-end example:

```toml
[dependencies]
airframe_http = { path = "../airframe_http", features = ["server", "client", "module"] }
```

## Usage

### Example 1: Start an Axum server module and expose a route

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use airframe_core::app::AppBuilder;
use airframe_http::axum_server::{AxumServerModule, RouterContributor};
use axum::{Router, routing::get};

// A simple contributor that mounts a hello route
struct Hello;
impl RouterContributor for Hello {
    fn mount(&self, router: Router) -> Router {
        router.route("/hello", get(|| async { "hello" }))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with(AxumServerModule::new("127.0.0.1:3000".parse::<SocketAddr>()?))
        .start()
        .await?;

    // Register our contributor so the server mounts it
    // The helper will create the registry if it doesn't exist yet
    let reg = airframe_http::axum_server::get_or_create_contrib_registry(&app.services);
    reg.add(Arc::new(Hello));

    // Retrieve the bound address (useful if port=0 was used)
    let addr = app.services.get::<airframe_http::axum_server::BoundAddr>().expect("addr").0;
    println!("server listening on http://{addr}");

    // Run until Ctrl+C or app.stop() is called elsewhere
    tokio::signal::ctrl_c().await?;
    app.stop().await?;
    Ok(())
}
```

### Example 2: Use the reqwest client module to make a GET request

```rust
use airframe_core::app::AppBuilder;
use airframe_http::prelude::*; // HttpClient, SpecClient, ReqwestClient

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register the reqwest-backed HttpClient into the ServiceRegistry
    let app = AppBuilder::new()
        .with(airframe_http::client_module::ReqwestClientModule::new())
        .start().await?;

    // Fetch the client by trait object from the registry
    let client = app.services.get::<dyn HttpClient<Error = reqwest::Error>>()
        .expect("http client");

    // Make a simple GET request
    let resp = client.get("https://httpbin.org/get").await?;
    println!("status: {}", resp.status());

    Ok(())
}
```

If you prefer not to use modules, you can directly construct `ReqwestClient` (feature `client`) and call methods on it.

## Status

APIs implemented and Airframe module interfaces implemented for server (Axum) and client (reqwest) behind feature flags. Some functionality is minimal and may evolve.

## License

This project is licensed under the repository license; see the top-level LICENSE file.
