### Goal
Identify other module interop combinations like airframe_health + airframe_http (via RouterContributor) that enhance each module’s feature set. Below you’ll find:
- Existing, working interops already present in the repo
- Additional high‑value combinations to add next, with short design notes (what capability seam to use and how to wire)
- Minimal code snippets to illustrate wiring where helpful

### What’s already wired today (use as patterns)
- Health + HTTP (Axum): `airframe_health` exposes probe routes via `airframe_http`’s `RouterContributor` seam when feature `adapters-axum` is enabled.
    - Feature gates: `airframe_health[adapters-axum]` and `airframe_http[server,module]`
    - Files: `airframe_health/src/http_axum.rs`, `airframe_http/src/server/router_contrib.rs`
- Admin + HTTP: `airframe_http::admin::AdminModule` contributes `/admin/*` routes via the same `RouterContributor` seam.
    - Capability: provides `cap:http.router.admin`, requires `cap:http.server`
    - File: `airframe_http/src/admin.rs`
- Health + resource modules: multiple resource providers optionally integrate with health when present.
    - `airframe_db`, `airframe_redis`, `airframe_secrets`, `airframe_winreg` declare `optional_requires: ["cap:health"]` and register probes when `cap:health` exists.
- Config + Args: `airframe_config` can be feature‑gated to require `cap:args` and parse CLI.
- Secrets + Crypt: `airframe_secrets` requires `cap:crypt` and optionally integrates with `cap:health`.
- PData/SData + Secrets: `airframe_pdata` optionally requires `cap:secrets`; `airframe_sdata` optionally requires `cap:pdata`.
- Scheduler + KV: `airframe_scheduler` uses `airframe_kv` for KV‑backed `JobSpec` and coordination.
- HTTP client interop: `airframe_http` exposes a spec‑driven client (`SpecClient`) that can call other HTTP providers (e.g., Admin API).

### High‑value interops to add next
These follow the same two interop seams used today:
- HTTP Router composition seam: `RouterContributor` (good for surfacing a module’s observability/admin endpoints)
- Capability hooks: `requires` / `optional_requires` in each `ModuleDescriptor` (good for enabling health probes, cross‑module discovery, etc.)

#### 1) Logging + HTTP: live log level control and recent logs
- Why: Enhances `airframe_logging` with runtime controls and quick diagnostics.
- How:
    - Add `LoggingAdminContributor` implementing `RouterContributor` to mount routes under `/admin/logging`.
    - Optional `cap:args` dependency for human‑friendly names if available (already used in logging as optional).
- Routes to add:
    - `GET /admin/logging/level` → current global/module levels
    - `POST /admin/logging/level` → set level(s)
    - `GET /admin/logging/recent` → ring buffer of recent events (keep in memory, feature‑gated)
- Module descriptor changes (logging module):
    - `optional_requires: ["cap:http.server"]` to register contributor when a server exists.

#### 2) Scheduler + HTTP: job introspection and control plane
- Why: Observe and control background jobs.
- How:
    - Add `SchedulerContributor` implementing `RouterContributor`, expose under `/admin/scheduler`.
    - If `cap:kv` is present, show KV‑backed schedules and revision/etag info.
- Routes:
    - `GET /admin/scheduler/jobs` → list in‑memory and KV‑backed jobs
    - `POST /admin/scheduler/jobs/{id}/run` → immediate trigger
    - `POST /admin/scheduler/jobs/{id}/pause|resume` → control
- Descriptor changes (scheduler):
    - `optional_requires: ["cap:http.server", "cap:kv"]`

#### 3) KV + HTTP: simple KV browser for dev/admin
- Why: Super useful for debugging systems leveraging `airframe_kv` or KV‑driven scheduler.
- How:
    - Add `KvAdminContributor` implementing `RouterContributor` under `/admin/kv`.
- Routes:
    - `GET /admin/kv/prefix?prefix=...` → list
    - `GET /admin/kv/key/{key}` → read (with `If-Match` reflection)
    - `PUT /admin/kv/key/{key}` and `DELETE ...` → write/delete (guarded by allowlist in config)
- Descriptor changes (kv module): `optional_requires: ["cap:http.server"]`

#### 4) DB + HTTP: connection pool stats and ping
- Why: Operational visibility; complements existing health probe.
- How:
    - Add `DbAdminContributor` implementing `RouterContributor` under `/admin/db`.
    - If using a pooled adapter, surface pool metrics.
- Routes:
    - `GET /admin/db/ping` → quick exec of `SELECT 1` (or backend‑appropriate)
    - `GET /admin/db/pool` → pool size, in‑use, waiters, timeouts
- Descriptor changes (db module): `optional_requires: ["cap:http.server"]`

#### 5) Redis + HTTP: cache stats and flush tools (guarded)
- Why: Ops debug for caching layers.
- How:
    - Add `RedisAdminContributor` under `/admin/redis`.
- Routes:
    - `GET /admin/redis/ping` → `PING`
    - `GET /admin/redis/info` → selected `INFO` sections
    - `POST /admin/redis/flush?mode=...` → `FLUSHDB`/`FLUSHALL` (config/role‑guarded)
- Descriptor: `optional_requires: ["cap:http.server"]`

#### 6) Secrets + HTTP: secrets health and key ring info (safe subset)
- Why: Validate KMS or crypto plumbing without exposing sensitive data.
- How:
    - `SecretsAdminContributor` under `/admin/secrets`.
- Routes:
    - `GET /admin/secrets/health` → runs the existing encrypt/decrypt probe
    - `GET /admin/secrets/providers` → list configured providers (names only)
- Descriptor: `optional_requires: ["cap:http.server"]`, already `requires: ["cap:crypt"]`

#### 7) Health + Admin consolidation
- Why: Bring health endpoints under the Admin tree for a single discovery surface.
- How:
    - Extend `AdminModule` to optionally mount a proxy route to health if `cap:health` is present (or just include links in `openapi.json`).
- Descriptor (admin): `optional_requires: ["cap:health"]` then compose or link.

#### 8) PData/SData + HTTP: quick transformers playground
- Why: Enable testing protected/structured data transformations.
- How:
    - `PDataContributor` and `SDataContributor` under `/admin/pdata` and `/admin/sdata`.
    - Only enabled in dev/test profiles to avoid misuse.
- Descriptor: `optional_requires: ["cap:http.server", "cap:secrets"]` (pdata) and `optional_requires: ["cap:http.server", "cap:pdata"]` (sdata)

#### 9) HTTP Client + Admin: turnkey CLI to call admin APIs
- Why: Self‑diagnose from within an app instance or companion CLI.
- How:
    - Reuse `airframe_http::SpecClient` with the `AdminModule::codespec()`.
    - Provide a small `AdminCliModule` (there’s already an example) that registers a `ValueProvider` for `admin.health` and commands like `admin.refresh`.

### Minimal wiring examples

#### Mounting a module’s routes via RouterContributor
```rust
use airframe_core::app::AppBuilder;
use airframe_http::axum_server::{AxumServerModule, mount_all};

let app = AppBuilder::new()
    .with(AxumServerModule::localhost_ephemeral())
    // add your module that registers a RouterContributor in its init()
    .with(YourModule::new())
    .build()
    .await?;

// after init, build the router by mounting all contributors
let reg = airframe_http::axum_server::get_or_create_contrib_registry(&app.services);
let router = mount_all(reg.all());
```

#### A skeleton Admin contributor for any module
```rust
use axum::{routing::get, Router};
use airframe_http::axum_server::RouterContributor;

pub struct MyAdminContributor;

impl RouterContributor for MyAdminContributor {
    fn mount(&self, router: Router) -> Router {
        router.route("/admin/myfeature/status", get(|| async { "ok" }))
    }
}
```

#### Registering the contributor from your module
```rust
use std::sync::Arc;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor};
use airframe_http::axum_server::get_or_create_contrib_registry;
use async_trait::async_trait;
use semver::Version;

pub struct MyModule { desc: ModuleDescriptor }

impl MyModule {
    pub fn new() -> Self {
        Self { desc: ModuleDescriptor {
            name: "my", version: Version::parse("0.1.0").unwrap(),
            provides: &["cap:my"], requires: &[],
            optional_requires: &["cap:http.server"],
            requires_with_versions: &[], optional_requires_with_versions: &[],
        }}
    }
}

#[async_trait]
impl Module for MyModule {
    fn descriptor(&self) -> &ModuleDescriptor { &self.desc }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        if ctx.services.get::<airframe_http::axum_server::BoundAddr>().is_some() {
            let reg = get_or_create_contrib_registry(&ctx.services);
            reg.add(Arc::new(MyAdminContributor));
        }
        Ok(())
    }
}
```

### Quick matrix of suggested combos and benefits
- airframe_logging + airframe_http: runtime logging control, recent logs
- airframe_scheduler + airframe_http (+ airframe_kv): job introspection/control
- airframe_kv + airframe_http: dev/admin KV browser
- airframe_db + airframe_http: DB pool stats + ping
- airframe_redis + airframe_http: cache stats + ping/flush
- airframe_secrets + airframe_http: probe and provider info
- airframe_admin + airframe_health: unify discovery/links for health under `/admin`
- airframe_pdata + airframe_http (+ airframe_secrets): protected data playground (dev only)
- airframe_sdata + airframe_http (+ airframe_pdata): structured transformers (dev only)

### How to decide what to implement first
- Highest ops value: DB/Redis/KV admin routes and Scheduler introspection
- Lowest risk: read‑only status routes (no mutation) under `/admin/*`
- Security posture: gate mutating routes with config/role checks and build separate admin listener if needed

### TL;DR
Use the two proven seams everywhere:
- Router composition: implement `RouterContributor` in each module that can expose ops/diagnostics, and register it in `Module::init` when `cap:http.server` is present.
- Optional capability hooks: declare `optional_requires` to pick up `cap:health`, `cap:http.server`, `cap:kv`, etc., and register probes or route contributors conditionally.

This yields immediate wins similar to `airframe_health` + `airframe_http`, and scales across logging, scheduler, KV, DB, Redis, secrets, PData/SData, and the existing Admin surface.