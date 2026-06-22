//! Prefab constructors (builders) for common app types.

use airframe_core::app::AppBuilder;

mod defaults;
pub use defaults::*;

/// Internal macro to eliminate boilerplate across prefab constructors.
///
/// Each invocation defines a unit struct with `new()` and `new_with_profile()` methods
/// that build an `AppBuilder` pre-wired with the given modules and config defaults.
///
/// The `finalize` parameter is a closure `|builder: AppBuilder| -> AppBuilder` that
/// receives the builder after all standard modules have been added. Use it for any
/// extra wiring (e.g., feature-gated post-modules, server binds). Pass `|b| b` for
/// a no-op.
macro_rules! define_prefab {
    (
        $(#[$meta:meta])*
        $doc:literal,
        $name:ident,
        $defaults_fn:expr,
        args: $args:tt,
        pre: [$($pre_mod:expr),* $(,)?],
        post: [$($post_mod:expr),* $(,)?],
        finalize: $finalize:expr
    ) => {
        #[doc = $doc]
        $(#[$meta])*
        pub struct $name;

        $(#[$meta])*
        #[allow(unused_mut)]
        impl $name {
            #[allow(clippy::new_ret_no_self)]
            pub fn new() -> AppBuilder {
                let mut builder = AppBuilder::new().with_bootstrap(airframe_core::app::Bootstrap {
                    minimal_logger: true,
                });
                // Pre-modules (before args/config/logging)
                $(
                    builder = builder.with($pre_mod);
                )*
                // Args (feature-gated)
                define_prefab!(@maybe_args builder, $args);
                // Config with base defaults (feature-gated)
                #[cfg(feature = "config")]
                {
                    builder = builder.with(
                        airframe_config::ConfigModule::new(None)
                            .with_defaults($defaults_fn()),
                    );
                }
                // Logging (feature-gated)
                #[cfg(feature = "logging")]
                {
                    builder = builder.with(airframe_logging::LoggingModule::new());
                }
                // Post-modules (after logging)
                $(
                    builder = builder.with($post_mod);
                )*
                // Finalize (custom per-prefab wiring)
                let finalize_fn: fn(AppBuilder) -> AppBuilder = $finalize;
                finalize_fn(builder)
            }

            /// Construct with a specific runtime profile; merges base defaults with profile tweaks.
            #[cfg(feature = "config")]
            pub fn new_with_profile(profile: super::prefabs::defaults::PrefabProfile) -> AppBuilder {
                let mut base = $defaults_fn();
                let prof = defaults::profile_defaults(profile);
                defaults::merge_toml(&mut base, prof);

                let mut builder = AppBuilder::new().with_bootstrap(airframe_core::app::Bootstrap {
                    minimal_logger: true,
                });
                // Pre-modules (before args/config/logging)
                $(
                    builder = builder.with($pre_mod);
                )*
                // Args (feature-gated)
                define_prefab!(@maybe_args builder, $args);
                // Config with merged defaults
                builder = builder.with(
                    airframe_config::ConfigModule::new(None).with_defaults(base),
                );
                // Logging (feature-gated)
                #[cfg(feature = "logging")]
                {
                    builder = builder.with(airframe_logging::LoggingModule::new());
                }
                // Post-modules (after logging)
                $(
                    builder = builder.with($post_mod);
                )*
                // Finalize (custom per-prefab wiring)
                let finalize_fn: fn(AppBuilder) -> AppBuilder = $finalize;
                finalize_fn(builder)
            }
        }
    };

    // Helper: conditionally emit the args block
    (@maybe_args $builder:ident, true) => {
        #[cfg(feature = "args")]
        {
            $builder = $builder.with(airframe_args::ArgsModule::new());
        }
    };
    (@maybe_args $builder:ident, false) => {};
}

// ---------------------------------------------------------------------------
// Non-HTTP prefabs
// ---------------------------------------------------------------------------

define_prefab! {
    "CLI Prefab\n\nPurpose: terminal application with config + logging ready to register CLI commands.",
    CliPrefab,
    defaults::cli,
    args: true,
    pre: [],
    post: [],
    finalize: |b| b
}

define_prefab! {
    "Service (daemon) Prefab\n\nPurpose: long-running process with logging, config, and health.",
    ServicePrefab,
    defaults::service,
    args: true,
    pre: [],
    post: [airframe_health::HealthModule::new()],
    finalize: |b| b
}

define_prefab! {
    "Worker Prefab (queue/event consumer)\n\nPurpose: Background consumer with logging, config, and health.",
    WorkerPrefab,
    defaults::worker,
    args: true,
    pre: [],
    post: [airframe_health::HealthModule::new()],
    finalize: |b| b
}

define_prefab! {
    "Scheduled Service Prefab (cron-like jobs)\n\nPurpose: Time-based scheduler with health.",
    ScheduledServicePrefab,
    defaults::scheduled,
    args: false,
    pre: [],
    post: [
        airframe_scheduler::SchedulerModule::new(),
        airframe_health::HealthModule::new()
    ],
    finalize: |b| b
}

// ---------------------------------------------------------------------------
// HTTP feature-dependent prefabs
// ---------------------------------------------------------------------------

#[cfg(feature = "http")]
use std::net::SocketAddr;

/// Finalize an HTTP prefab builder: add optional OpenAPI module, then bind the Axum server
/// to an ephemeral port (avoids test/CI port clashes; override via config layer).
#[cfg(feature = "http")]
fn finalize_http(mut builder: AppBuilder) -> AppBuilder {
    #[cfg(feature = "openapi")]
    {
        builder = builder.with(crate::http_openapi::OpenApiModule::new());
    }
    let bind: SocketAddr = "127.0.0.1:0".parse().expect("valid localhost addr");
    builder.with(airframe_http::axum_server::AxumServerModule::new(bind))
}

/// Finalize an HTTP prefab builder without OpenAPI: bind the Axum server to an ephemeral port.
#[cfg(feature = "http")]
fn finalize_http_no_openapi(builder: AppBuilder) -> AppBuilder {
    let bind: SocketAddr = "127.0.0.1:0".parse().expect("valid localhost addr");
    builder.with(airframe_http::axum_server::AxumServerModule::new(bind))
}

define_prefab! {
    #[cfg(feature = "http")]
    "HTTP API Server Prefab (requires feature = \"http\")",
    HttpApiServerPrefab,
    defaults::http_api,
    args: true,
    pre: [
        airframe_health::HealthModule::new(),
        crate::http_cors::HttpCorsModule::new()
    ],
    post: [],
    finalize: finalize_http
}

define_prefab! {
    #[cfg(feature = "http")]
    "Gateway Prefab (reverse proxy/gateway) -- requires feature = \"http\"",
    GatewayPrefab,
    defaults::gateway,
    args: true,
    pre: [
        airframe_health::HealthModule::new(),
        crate::http_cors::HttpCorsModule::new(),
        crate::gateway::GatewayModule::new()
    ],
    post: [],
    finalize: finalize_http_no_openapi
}
