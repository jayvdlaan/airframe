# airframe_admin — Implementation Checklist

Legend: [ ] = TODO, [*] = In progress, [x] = Done

Scope: Build a reusable Airframe module that layers on top of airframe_http to provide a secure, extensible admin surface. The module relies on the host application to supply authentication/authorization.

---

## Phase 0 — Scaffold
- [ ] Create crate `airframe_admin` under `crates/airframe_admin/` with Cargo metadata and README.
- [ ] Add `lib.rs` with feature gates and top-level exports.
- [ ] Define `AdminConfig` and `AdminModule` descriptor (provides `cap:admin`, requires `cap:http.server`).
- [ ] Hook into Airframe’s `RouterContributor` seam to mount a base admin router at configurable `base_path` (default `/admin`).

## Phase 1 — Auth and guard
- [ ] Define principal and auth contracts:
  - [ ] `AdminPrincipal { subject, roles, scopes }`.
  - [ ] `AdminAuth` trait with `authenticate(&Parts) -> Result<AdminPrincipal, AdminAuthError>`.
  - [ ] `AdminAuthError` enum (Unauthorized, Forbidden, Invalid, Transient).
- [ ] Implement `AdminScope` enum with helper to map to required scopes.
- [ ] Implement Axum extractor `AdminGuard<SCOPE>` that uses injected `Arc<dyn AdminAuth>` from `ServiceRegistry`.
- [ ] Behavior when no `AdminAuth` is registered: return `501 Not Implemented` with redacted error on all admin routes.

## Phase 2 — Core routes and redaction
- [ ] Add core endpoints (mounted under `base_path`):
  - [ ] `GET /admin` → discovery: module info, sections, actions (scope: ReadOnly).
  - [ ] `GET /admin/whoami` → current principal (scope: ReadOnly).
  - [ ] `GET /admin/healthz` → proxy or synthesize (scope: ReadOnly) when `cap:health` present.
  - [ ] `GET /admin/metrics` → proxy metrics (scope: ReadOnly) when `cap:metrics` present.
  - [ ] `GET /admin/config` → redacted config dump (scope: Superuser).
  - [ ] `POST /admin/reloads/config` → request config reload (scope: Superuser).
- [ ] Centralize error handling:
  - [ ] `AdminError` enum + renderer that redacts internal details.
  - [ ] Structured logs via `tracing` (method, path, status, subject, elapsed_ms; no bodies).

## Phase 3 — Actions registry
- [ ] Define `AdminActionSpec { id, scope, input_schema, description }`.
- [ ] Define `AdminAction` trait with `spec()` and `invoke(input: Value) -> Result<Value>`.
- [ ] Provide in-memory `AdminActionRegistry` service (list/get/register) and register default instance if none present.
- [ ] Endpoints:
  - [ ] `GET /admin/actions` → list specs (scope: ReadOnly).
  - [ ] `POST /admin/actions/:id` → invoke action (scope: from spec). Body-size limit applied.

## Phase 4 — Sections contributor
- [ ] Define `AdminSectionContributor` trait:
  - [ ] `fn section(&self) -> &'static str`.
  - [ ] `fn mount(&self, router: axum::Router) -> axum::Router`.
  - [ ] `fn required_scope(&self) -> AdminScope` (default Operator).
- [ ] Register and auto-mount all contributed sections under `/admin/:section/...` using a small registry in `ServiceRegistry`.
- [ ] Provide helper `register_section(&ServiceRegistry, Arc<dyn AdminSectionContributor>)`.

## Phase 5 — Hardening and middleware
- [ ] Body-size limits: default 256 KiB for admin; allow contributor override.
- [ ] Per-principal token-bucket rate limiting (configurable `rps` and `burst`); strict bucket for `POST /admin/actions/*`.
- [ ] IP allowlist enforcement (optional) based on remote addr (and/or trusted proxy headers if enabled).
- [ ] TLS requirement toggle; reject non-HTTPS when enabled (best-effort behind proxies).
- [ ] Audit sink:
  - [ ] `AdminAuditEvent { when, subject, action, status, details }`.
  - [ ] `AdminAuditSink` trait; default logs to `tracing`; apps can register durable sink.

## Phase 6 — OpenAPI and docs
- [ ] If `cap:openapi` is present, contribute admin schemas and paths to OpenAPI document.
- [ ] Provide `AdminOpenApiContributor` extension hook so sections can add their fragments.
- [ ] Write crate README with usage, security guidance, and examples (auth plugin, actions, section contrib).

## Phase 7 — Tests and examples
- [ ] Unit tests:
  - [ ] `AdminGuard` auth/deny paths.
  - [ ] Rate limiter behavior.
  - [ ] Actions registry list/get/invoke.
- [ ] Integration test:
  - [ ] Mount a demo app with dummy `AdminAuth`; hit core routes and a test action.
- [ ] Example code:
  - [ ] Minimal app wiring `AdminModule` + custom `AdminAuth` + one action and one section.

---

## Configuration (AdminConfig)
- [ ] `enabled: bool` (default true).
- [ ] `base_path: String` (default "/admin").
- [ ] `require_tls: bool` (default true in non-dev).
- [ ] `ip_allowlist: Option<Vec<Cidr>>`.
- [ ] `rate_limit: { requests_per_sec: f64, burst: f64 }` (default 2 rps / 6 burst).
- [ ] `actions_rate_limit: { requests_per_sec: f64, burst: f64 }` (default 1 rps / 3 burst).
- [ ] `body_limit_bytes: usize` (default 262_144).
- [ ] `openapi: bool` (default on when `cap:openapi`).
- [ ] `redact_errors: bool` (default true).

---

## Security guidance (to document and enforce)
- [ ] Do not ship with admin enabled in public builds without an `AdminAuth` registered.
- [ ] Encourage implementers to put admin behind TLS and an IP allowlist.
- [ ] Never log request bodies or secrets; redact errors by default.
- [ ] Provide audit hooks and recommend durable storage for audit logs in production.

---

## Adoption checklist for an application
- [ ] Add `airframe_admin::AdminModule` to the app’s AppBuilder after HTTP server.
- [ ] Provide and register an `Arc<dyn AdminAuth>` implementation.
- [ ] Optionally register actions via `AdminActionRegistry`.
- [ ] Optionally register sections via `AdminSectionContributor`.
- [ ] (If using OpenAPI) register admin OpenAPI contributor fragments.
- [ ] Harden deployment: TLS, IP allowlist, rate limits, audit sink.
