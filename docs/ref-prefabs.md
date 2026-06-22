# Airframe Prefabs — Implementation Plan and Checklists

Last updated: 2026-01-28

This document tracks the scoped implementation plan for the first six prefabs.

## Implementation Status

| Prefab | Status | Notes |
|--------|--------|-------|
| CLI | **Complete** | All features implemented |
| Service (daemon) | **Complete** | All features implemented |
| HTTP API Server | **Complete** | All features implemented |
| Worker | **Complete** | Minor doc updates pending |
| Proxy/Gateway | **Scaffold Only** | Routing, rate limiting, hot-reload not yet implemented |
| Scheduled Service | **Scaffold Only** | Job scheduling, leader election not yet implemented |

It provides a high-level description and concrete checklists per prefab so progress is visible and reviewable.

---

## Global checklist (cross‑cutting)
- [x] Common scaffold helpers (Prefab builders returning an AppBuilder)
- [ ] Capability descriptors aligned (provides/requires/optional_requires)
- [x] Config layering: defaults → file → env → args
- [x] Bootstrap: prefabs with logging enable minimal early stderr logger
- [ ] Security defaults: admin bound to 127.0.0.1, mutating admin routes disabled by default
- [ ] Observability defaults: logs, health, metrics (feature‑gated)
- [ ] Example apps for each prefab under `examples/` (runnable with minimal config)
- [ ] CI: format, lint, and tests green across workspace

### Bootstrap behavior
Prefabs that include logging install a minimal stderr logger during the bootstrap phase (before modules initialize). This captures very‑early logs such as "app starting". The bootstrap logger is best‑effort and is superseded once the full logging module initializes. To capture these early logs, redirect stderr (for example, run your app with `2>logs.txt`).

---

## 1) CLI Prefab
Purpose: Command‑line entry point with subcommands, shared config, and observability.

Default modules
- airframe_logging, airframe_config, airframe_args

Optional modules
- airframe_http::client, airframe_secrets, airframe_crypt, airframe_kv

Capabilities
- Provides: `cap:cli`
- Optional requires: `cap:crypt` (when secrets enabled)

Config surface
- app.name, app.version
- logging.level (info), logging.format (compact|json)
- `--config <path>` and env layering (`APP__NESTED__KEY`)

Usage
- Run the example:
  - `cargo run -p airframe_prefab --example cli -- hello World 2>logs.txt`
  - Global flags before `--` are parsed early: `--quiet | --verbose | --json`
  - Redirect stderr to capture early bootstrap logs: `2>logs.txt`
- Subcommands are registered via `CliRegistry` and dispatched based on the first arg.

Config examples
- TOML file (app.toml):
  ```toml
  [logging]
  directives = ["info"]
  [[logging.sinks]]
  kind = "console"
  json = false
  ansi = true
  stderr = true
  ```
- Run with config file:
  - `cargo run -p airframe_prefab --example cli -- --config ./app.toml hello beans`
- Or via env (layered):
  - `RUST_LOG=info cargo run -p airframe_prefab --example cli -- hello`

Checklist
- [x] Scaffold: `CliPrefab::new() -> AppBuilder`
- [x] Subcommand registration/dispatch API (CliCommand + CliRegistry)
- [x] Logging flags: `--quiet/--verbose`, JSON toggle
- [x] Example: `examples/cli` with 2–3 demo subcommands (skeleton)
- [x] Tests: dispatch
- [x] Tests: config layering precedence
- [x] Docs: README section with usage and config examples

---

## 2) Service (daemon) Prefab
Purpose: Long‑running process with graceful shutdown, admin surface, and health/metrics.

Default modules
- airframe_logging, airframe_config, airframe_health

Optional modules
- airframe_http::server (admin), airframe_metrics, airframe_secrets, airframe_scheduler

Capabilities
- Provides: `cap:service`
- Optional requires: `cap:http.server`, `cap:metrics`, `cap:health`

Config surface
- admin.enable (bool, default true)
- server.bind (default 127.0.0.1:8080 for admin)
- logging.*, shutdown.grace_period (e.g., 30s)

Usage
- Run the service example (if present) or wire your own main using `ServicePrefab::new()`.
- Admin listener defaults to localhost only; to expose beyond localhost, set `server.bind` explicitly.
- Mutating admin routes should remain disabled by default; enable them explicitly in config when available.
- Graceful shutdown: send SIGINT (Ctrl+C) or SIGTERM; the app drains up to `shutdown.grace_period`.

Checklist
- [x] Scaffold: `ServicePrefab::new() -> AppBuilder`
- [x] Graceful shutdown (SIGINT/SIGTERM), drain tasks within grace period
- [x] Admin routes mounted when HTTP is present
- [x] Example: `examples/service`
- [x] Tests: shutdown behavior; health endpoint exposure
- [x] Docs: systemd/Windows Service notes, admin routes

---

## 3) HTTP API Server Prefab
Purpose: Build REST/JSON APIs quickly with admin/observability included.

Default modules
- airframe_logging, airframe_config, airframe_http::server, airframe_health

Optional modules
- airframe_metrics, airframe_db, airframe_redis, airframe_kv, airframe_secrets, airframe_auth

Capabilities
- Provides: `cap:http.server`, optional `cap:openapi`

Config surface
- server.bind, server.tls.*
- cors.*, admin.enable

Usage
- Run the example (requires `http` feature):
  - `cargo run -p airframe_prefab --features http --example http_api 2>logs.txt`
  - The server binds to an ephemeral port by default (127.0.0.1:8080). The bound port is logged; for stable port, set `server.bind` in config.
- Test the ping route:
  - `curl -i http://127.0.0.1:8080/v1/ping`
  - If using the default ephemeral port, replace 8080 with the logged port.
- Enable CORS in config:
  ```toml
  [cors]
  enable = true
  allow_methods = ["GET", "POST", "OPTIONS"]
  allow_headers = ["Content-Type"]
  max_age = 600
  ```
- TLS (conceptual wiring): set `server.tls.*` keys when available in the HTTP module; not implemented here yet.

Checklist
- [x] Scaffold: `HttpApiServerPrefab::new() -> AppBuilder`
- [x] Router composition via RouterContributor, example `ApiModule`
- [x] Optional OpenAPI placeholder wiring
- [x] CORS configuration surface
- Config keys: cors.enable (bool), cors.allow_origins ("*" or [..]), cors.allow_methods ([..]), cors.allow_headers ([..]), cors.max_age (secs)
- [x] Example: `examples/http_api`
- [x] Tests: route registration
- [x] Tests: CORS
- [x] Tests: basic error mapping
- [x] Docs: curl examples, bind/TLS config
- [x] Stabilized OpenAPI endpoint registration (eliminated router build race)

---

## 5) Worker Prefab (queue consumer)
Purpose: Consume jobs/events from a queue with concurrency, retries, and observability.

Default modules
- airframe_logging, airframe_config, airframe_health

Optional modules
- airframe_metrics, airframe_kv (offsets), queue adapters (kafka|nats|sqs|redis_streams), airframe_secrets

Capabilities
- Provides: `cap:worker`

Config surface
- queue.* (bootstrap, topic/subject), concurrency, retry.backoff, dlq.*
- Admin: `/admin/worker/*` when HTTP server present

Usage
- Run the worker example (if present):
  - `cargo run -p airframe_prefab --example worker 2>logs.txt`
- Configure retry and DLQ behavior in config; metrics and health integrate when those modules are present.

Checklist
- [x] Scaffold: `WorkerPrefab::new() -> AppBuilder`
- [x] Handler registration API and concurrency controls
- [x] Retry strategy with jitter; DLQ optional wiring
- [x] Example: `examples/worker` (dev adapter)
- [x] Tests: at‑least-once semantics, retry, graceful drain
- [x] Docs: basic usage
- [ ] Docs: adapter notes, metrics/health

---

## 8) Proxy/Gateway Prefab

> **STATUS: SCAFFOLD ONLY** — Basic structure exists but core functionality (routing, rate limiting, hot-reload) is not yet implemented.

Purpose: Reverse proxy with routing, rate limiting, and optional auth.

Default modules
- airframe_logging, airframe_config, airframe_http::server, airframe_metrics

Optional modules
- airframe_kv (counters/limits), airframe_redis (rate limit buckets), airframe_secrets (keys), airframe_http::client

Capabilities
- Provides: `cap:http.server`, `cap:gateway`

Config surface
- routes[] (matchers, upstreams), timeouts, limits, auth.*, cors.*
- Hot‑reload from config file or KV prefix

Checklist
- [x] Scaffold: `GatewayPrefab::new() -> AppBuilder`
- [ ] Routing table loader + matcher; proxy via HTTP client pool
- [ ] Rate limiting (local + Redis option)
- [ ] Hot‑reload (file/KV) with zero‑drop of in‑flight requests
- [ ] Example: `examples/gateway`
- [ ] Tests: routing, headers/timeouts, rate limit logic
- [ ] Docs: route schema, auth plug points

---

## 6) Scheduled Service Prefab

> **STATUS: SCAFFOLD ONLY** — Basic structure exists but core functionality (job scheduling, leader election, admin controls) is not yet implemented.

Purpose: Time‑based job runner (cron‑like) with KV coordination and admin controls.

Default modules
- airframe_logging, airframe_config, airframe_scheduler, airframe_health

Optional modules
- airframe_http::server (admin), airframe_metrics, airframe_kv (leader election/checkpoints)

Capabilities
- Provides: `cap:scheduler`
- Optional requires: `cap:kv`, `cap:http.server`

Config surface
- scheduler.jobs[] (id, schedule, handler), scheduler.timezone
- leader_election (bool), checkpoint.kv_prefix

Checklist
- [x] Scaffold: `ScheduledServicePrefab::new() -> AppBuilder`
- [ ] Jobs registration + deterministic scheduling
- [ ] Leader election and checkpointing when KV present
- [ ] Admin controls: list/pause/resume/run‑now (gated)
- [ ] Example: `examples/scheduled_service`
- [ ] Tests: next run accuracy, pause/resume, singleton enforcement
- [ ] Docs: cron expression guide, profiles

---


## Notes and guardrails
- Prefer optional capability hooks (`optional_requires`) so features light up when present, without hard coupling.
- Admin listeners should bind to localhost by default; any mutating routes must be explicitly enabled via config and, where applicable, roles/allowlists.
- Observability: compact logs in dev, JSON logs in prod; expose `/admin/health` and `/admin/metrics` when enabled.
