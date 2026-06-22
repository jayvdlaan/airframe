# airframe_api

Base types and traits for public APIs within Airframe.

## Overview

`airframe_api` provides minimal, runtime-agnostic building blocks for describing
and using HTTP APIs. It deliberately stays small and does not prescribe a
parameter model, transport, or framework.

The public surface is:

- `Endpoint` — a `struct` describing a single endpoint: `operation_id`, `method`
  (`http::Method`), and a `path_template` such as `"/v1/items/{id}"`.
- `ApiSpec` — a `Send + Sync` trait for resolving endpoints and constructing
  requests. It exposes `base_uri()`, `endpoint(operation_id)`, and
  `build_request(operation_id, params, body)`, where `params` is a
  `serde_json::Value` so no specific parameter model is imposed at this layer.
- `CodeSpec` — a code-first `ApiSpec` implementation built programmatically via
  `CodeSpec::new(base)` and the chainable `route(operation_id, method, path_template)`.
  Its `build_request` performs `{name}` path-template expansion from `params`,
  appends a query string from `params["query"]` when present, and URL-encodes
  values.
- `is_valid_path_template(&str) -> bool` — a lightweight validator that checks
  `{`/`}` braces are balanced and not nested (flat templates only). It does not
  validate parameter names.
- `ApiError` — a portable, JSON-serializable error type with a numeric `code`
  (HTTP semantics where possible) and an optional `message`, plus the helpers
  `is_client_error()` (400–499) and `is_server_error()` (500–599).

The crate also re-exports `bytes` and `http` for convenience.

## Dependencies

- `http` — `Method`, `Request`, `Uri`
- `bytes` — request bodies as `Bytes`
- `serde` / `serde_json` — `params` model and `ApiError` (de)serialization
- `anyhow` — error type for `build_request`
- `urlencoding` — path and query value encoding
- `tracing` — request-building diagnostics

System libraries: none. This crate defines API contracts and does not itself
implement an Airframe module.

## Usage

```rust
use airframe_api::{ApiSpec, CodeSpec, is_valid_path_template};
use airframe_api::http::{Method, Uri};

let base: Uri = "https://api.example.com".parse().unwrap();
assert!(is_valid_path_template("/v1/items/{id}"));

let spec = CodeSpec::new(base)
    .route("list_items", Method::GET, "/v1/items")
    .route("get_item", Method::GET, "/v1/items/{id}");

// Resolve an endpoint by operation id.
let ep = spec.endpoint("get_item").unwrap();
assert_eq!(ep.path_template, "/v1/items/{id}");

// Build a request, expanding the `{id}` path parameter.
let params = serde_json::json!({ "id": "123" });
let req = spec.build_request("get_item", &params, None).unwrap();
assert_eq!(req.uri().to_string(), "https://api.example.com/v1/items/123");
```

## Status

Pre-release (`0.5.0-beta`). The public surface is small and may still evolve.

Licensed under MIT.
