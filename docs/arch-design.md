Airframe architecture design (L0–L7 layering)

Overview
This document defines the layering model used across the Airframe workspace. The goal is to keep the runtime module graph acyclic, minimize coupling between crates, and provide clear rules about which crates may depend on which. The layers also inform how ModuleDescriptor dependencies are declared versus when to rely on runtime lookups via the ServiceRegistry.

Key rule
- Dependencies must only point downward: A crate/module at layer N may depend on layers ≤ N. Never depend upward on a higher layer.
- This rule applies to both build-time (Cargo) dependencies and runtime ModuleDescriptor edges (requires and optional_requires). Optional edges still count for topology and layering checks.

Layers at a glance
- L0 Core kernel
  - Crates: airframe_core
  - Role: Module system, ServiceRegistry, in-memory buses, app bootstrap.

- L1 Primitives (no IO)
  - Crates: airframe_codec, airframe_compress, airframe_crypt, airframe_api, airframe_macros, airframe_log_api, airframe_id, airframe_wire, airframe_net, airframe_channel, airframe_pdata (pure), airframe_sdata (pure)
  - Role: Pure utilities and types; protocol/identity primitives; no files, OS sockets, or OS-specific IO.
  - Notes: airframe_macros provides derive/declarative helpers (depends only on core). airframe_log_api is a dependency-free logging facade (the Logger trait), distinct from the L3 logging runtime. airframe_id and airframe_wire are dependency-free identifier and bit-level binary primitives. airframe_net (reliable-UDP primitives) and airframe_channel (Noise_XX secure channel, on airframe_crypt) supply transport primitives without declaring runtime module edges.

- L2 Config and Args
  - Crates: airframe_config, airframe_args
  - Role: Load/merge configuration from files/env/CLI; expose BasicConfig; parse CLI.
  - Constraint: Must not depend on logging or HTTP. Reads CLI opportunistically.

- L3 Logging
  - Crates: airframe_logging
  - Role: Runtime logging wired to tracing; reads config (L2) when feature enabled.
  - Constraint: Nothing in L2 may depend back on L3.

- L4 Runtime adapters and IO
  - Crates: airframe_http, airframe_kv, airframe_data, airframe_db, airframe_secrets, airframe_scheduler, airframe_event (IO), airframe_health, airframe_redis, airframe_sqlite, airframe_mysql, airframe_pg, airframe_winreg, airframe_audit, airframe_metrics, airframe_tabular, airframe_ipc
  - Role: Concrete IO providers (servers, storage, schedulers, health checks, audit logs, metrics exposition, tabular ingest, IPC, OS integration).
  - Constraint: Do not require higher layers (L5+). Prefer reading config at runtime without encoding descriptor edges, unless strictly required for init ordering.

- L5 Integrations / prefab features
  - Crates: airframe_prefab (e.g., http_openapi), airframe_recovery_bundle
  - Role: Plug-in integrations and higher-level composed features. airframe_prefab requires capabilities from L4 (e.g., cap:http.server); airframe_recovery_bundle composes the recovery-bundle format and K-of-N share-combining for the ceremony framework (format-only; AEAD is performed by callers).

- L6 Domain services and libraries
  - Crates (outside airframe/): nanokey, nanopass, nanokey_client, libnanokey
  - Role: Product/domain-specific HTTP modules, handlers, adapters.

- L7 Applications / binaries
  - Binaries: nanokey (bin), nanopass (bin)
  - Role: Final composition of modules, features, and process lifecycle.

Allowed dependency directions
- Build-time (Cargo): A crate may depend only on the same or a lower layer crate.
- Runtime (ModuleDescriptor): For any edge A -> B (A requires or optionally requires a capability provided by B), layer(A) ≥ layer(B) must hold.

Examples: correct vs incorrect
- Correct: logging (L3) requires cap:config (L2). This is downward.
- Incorrect: config (L2) optional_requires cap:logging (L3). This would create an upward edge and is disallowed.
- Correct: prefab-openapi (L5) requires cap:http.server (L4).
- Incorrect: http-axum-server (L4) optional_requires cap:logging (L3) or cap:config (L2) in its descriptor. The server can still read BasicConfig from the registry at init time without a declared edge.
- Correct: domain HTTP modules (L6) optionally require cap:http.server (L4) to mount routes via a RouterContributor seam.

Guidance: ModuleDescriptor edges vs runtime lookups
- Use requires when:
  - Your module cannot initialize without a capability and there must be an enforced init ordering (e.g., logging requires config to build sinks).
- Use optional_requires sparingly when:
  - You genuinely need the other module initialized first if present, but can proceed without it. Remember: optional edges still affect topology and layering checks.
- Prefer runtime lookups via ServiceRegistry when:
  - You want to read a service opportunistically (e.g., BasicConfig) but don’t need to enforce an initialization order. This avoids unnecessary edges that can contribute to cycles.

Feature flags and variants
- Be consistent across feature branches. A descriptor that drops or adds edges behind cfg(features) must still respect layering in all combinations.
- CI should build commonly used feature matrices to catch violations (see CONTRIBUTING.md).

Layering validator
- The airframe_core crate provides an optional startup validator (feature: layer-check) that inspects the module graph at runtime and rejects upward edges with a clear error, e.g.:
  layer violation: config (L2) depends on logging (L3)
- This check is opt-in for tests and developer builds:
  cargo test -p airframe_core --features layer-check

Cheat sheet: module naming and layers
- L0: core => "core"
- L1: crypt/codec/compress/api/macros/log_api/id/wire/net/channel/pdata/sdata => "crypt", "codec", "compress", "api", "pdata", "sdata" (macros/log_api/id/wire/net/channel are library primitives without descriptors)
- L2: args/config => "args", "config"
- L3: logging => "logging"
- L4: http-axum-server/kv/data/db/secrets/scheduler/event/health/redis/sqlite/mysql/pg/winreg/audit/metrics/tabular/ipc => names match provides in descriptors
- L5: prefab-openapi/recovery_bundle => "prefab-openapi" (recovery_bundle is a library, no descriptor)
- L6: nanokey-http/nanopass.http => "nanokey-http", "nanopass.http"

Anti-patterns to avoid
- Adding cap:logging as an optional dependency in lower layers (L1/L2/L4). Log via tracing or a minimal facade without declaring a module edge.
- Making the HTTP server depend (even optionally) on config/logging; prefer runtime ServiceRegistry lookups.
- Letting optional edges accumulate for convenience. Each one still participates in cycle checks and layer validation.

Migration notes (what we enforce today)
- ConfigModule no longer references cap:logging in any feature path.
- AxumServerModule has no optional dependencies in its descriptor; it reads configuration at init time.
- prefab-openapi requires only cap:http.server.
- Domain HTTP modules depend at most optionally on cap:http.server.

Appendix: glossary
- Capability: A string in ModuleDescriptor.provides (e.g., "cap:http.server").
- RouterContributor: A seam provided by airframe_http that allows modules to mount routes without the HTTP server depending on them.
- ServiceRegistry: The runtime registry used to look up services at init/start.
