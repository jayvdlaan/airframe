### Airframe mobile (Android/iOS) compatibility policy

This document describes how Airframe enforces module compatibility on mobile targets and what is currently supported.

#### What “supported on mobile” means
For a module/crate, there are three different states:

1. **Compiles** on Android/iOS (no unsupported syscalls/OS APIs at compile time)
2. **Supported** on Android/iOS (Airframe will allow it to be initialized)
3. **Meaningful** on Android/iOS (fits mobile lifecycle and product constraints)

Airframe enforces (2) at runtime via a **fail-fast platform preflight**.

### Fail-fast platform preflight (hard error)

`airframe_core::app::AppBuilder::start()` checks every selected module’s declared platform support via:

- `airframe_core::module::Module::platform_support()`

If a module is not supported on the current platform, startup fails **before any module init**, with an error like:

```
module "<name>" is not supported on <platform>: <reason>
```

This prevents “half-initialized” runtimes and avoids confusing runtime failures later.

### Current mobile support matrix (runtime modules)

The table below reflects the current policy encoded in `platform_support()` and mobile config guards.

| Crate / module | Mobile status | Notes |
|---|---:|---|
| `airframe_core` | Supported | Runtime + module system; platform preflight lives here. |
| `airframe_config::ConfigModule` | Supported (with restrictions) | Hot reload is **disabled by default** on Android/iOS and is rejected if explicitly enabled. |
| `airframe_logging::LoggingModule` | Supported (with restrictions) | Rejects `syslog` sink on Android/iOS. Rejects `journald` sink on non-Linux. |
| `airframe_health::HealthModule` | Supported | In-process health/readiness makes sense on mobile. Defaults are mobile-friendly (slower polling on Android/iOS). |
| `airframe_args::ArgsModule` | **Desktop-only** | CLI argv is not a meaningful surface on mobile. |
| `airframe_http::AxumServerModule` | **Desktop-only** | In-process HTTP server is not supported on mobile (lifecycle/background/battery constraints). |
| `airframe_http::AdminModule` | **Desktop-only** | Depends on in-process HTTP server. |
| `airframe_prefab` HTTP prefabs (SPA/static/openapi/cors) | **Desktop-only** | Depend on in-process HTTP server. |
| `airframe_prefab::GatewayModule` | **Desktop-only** | Depends on in-process HTTP server. |
| `airframe_prefab::WorkerModule` | **Desktop-only** | Designed for long-running background worker loops. |
| `airframe_scheduler::SchedulerModule` | **Desktop-only** | Designed for long-lived timers/watchers. |
| `airframe_redis::RedisModule` | **Desktop-only** | External Redis dependency; server-side module. |
| `airframe_db::DbModule` | Supported (with restrictions) | On Android/iOS, `db.driver=mysql` is rejected; use sqlite or a server-side DB service. |
| `airframe_kv::KvModule` | Supported (with restrictions) | On Android/iOS, `kv.backend=filesystem` is rejected (needs mobile storage adapter). |
| `airframe_cryptex_smartcard` | Supported only with `mock` | `feature = "pcsc"` is rejected on Android/iOS at compile time (no PC/SC stack). Extracted to `airframe-cryptex` workspace. |

### Mobile filesystem guidance (pdata/kv/etc.)

Several crates expose filesystem-backed storage backends (e.g., `airframe_pdata`’s `FsBackendSecure` helpers, or `airframe_kv`’s `filesystem` backend behind feature flags). On Android/iOS:

- Always use **app-private storage** locations provided by your host runtime (e.g., `filesDir`/`app data dir`) rather than assuming desktop-like paths (such as `./var/...`).
- Prefer passing an **explicit root directory** from the app (Tauri/mobile host) rather than selecting filesystem backends implicitly from env/config.

### Notes for application authors

- If you want a single codebase that builds for desktop + mobile, prefer:
  - gating desktop-only modules with `#[cfg(not(any(target_os = "android", target_os = "ios")))]`
  - or using feature flags to avoid pulling in unsupported backends.

- Some incompatibilities are *configuration-level*, not module-level (e.g. logging sinks). In those cases the module remains supported, but invalid configs are rejected with a clear error.
