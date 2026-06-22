//! airframe_http: Core HTTP traits and spec-driven client facade.
//! Facade crate that re-exports split submodules (api, clients, server) with feature gating.

pub use bytes;
pub use http;

// Public API modules
pub mod api {
    pub mod client;
    pub mod spec_client;
}

// Wire source files and provide stable public paths under `clients::*`
#[cfg(all(feature = "client", feature = "module"))]
#[path = "client/client_module.rs"]
mod client_client_module;
#[cfg(feature = "client")]
#[path = "client/reqwest.rs"]
mod client_reqwest;

// Concrete clients (public namespace)
pub mod clients {
    #[cfg(feature = "client")]
    pub mod reqwest {
        pub use crate::client_reqwest::*;
    }
    // Reqwest client Module behind client+module features
    #[cfg(all(feature = "client", feature = "module"))]
    pub mod client_module {
        pub use crate::client_client_module::*;
    }
}

// Server-side integrations
pub mod server {
    #[cfg(feature = "server")]
    pub mod axum_server;
    // Bind-address resolution helpers (internal; surfaced via axum_server).
    #[cfg(feature = "server")]
    pub(crate) mod bind;
    // AxumServerModule lives here; re-exported via `axum_server` for path stability.
    #[cfg(all(feature = "server", feature = "module"))]
    pub(crate) mod module;
    #[cfg(feature = "server")]
    pub mod router_contrib;
}

// Root-level re-exports for ergonomic use and to preserve existing example paths
pub use api::client::{HttpClient, InvokeError};
pub use api::spec_client::SpecClient;

// Keep the historical path airframe_http::reqwest_client::ReqwestClient
#[cfg(feature = "client")]
pub mod reqwest_client {
    pub use crate::clients::reqwest::ReqwestClient;
}

// Compatibility re-export to keep airframe_http::client_module::ReqwestClientModule working
#[cfg(all(feature = "client", feature = "module"))]
pub mod client_module {
    pub use crate::clients::client_module::*;
}

// Keep the historical path airframe_http::axum_server::{AxumServer, BoundAddr, ...}
#[cfg(feature = "server")]
pub mod axum_server {
    #[cfg(feature = "module")]
    pub use crate::server::axum_server::AxumServerModule;
    pub use crate::server::axum_server::{AxumServer, BoundAddr};
    #[cfg(feature = "module")]
    pub use crate::server::router_contrib::get_or_create_contrib_registry;
    #[cfg(feature = "module")]
    pub use crate::server::router_contrib::get_or_create_registry;
    pub use crate::server::router_contrib::{
        get_or_create_error_mapper_registry,
        get_or_create_gateway_header_policy_registry,
        get_or_create_gateway_rewriter_registry,
        get_or_create_health_registry,
        get_or_create_layers_registry,
        get_or_create_metrics_registry,
        mount_all,
        ErrorMapperRegistry,
        GatewayHeaderPolicy,
        GatewayHeaderPolicyRegistry,
        GatewayRewriter,
        GatewayRewriterRegistry,
        // registries
        GlobalLayerRegistry,
        HealthContribRegistry,
        MetricsHookRegistry,
        OrderedRouterContributor,
        RouterContribRegistry,
        RouterContributor,
        RouterPhase,
        VecRegistry,
    };
}

// Admin module remains available behind server+module features
#[cfg(all(feature = "server", feature = "module"))]
pub mod admin;

// Convenience prelude for common imports
pub mod prelude {
    pub use crate::api::client::{HttpClient, InvokeError};
    pub use crate::api::spec_client::SpecClient;
    #[cfg(feature = "client")]
    pub use crate::clients::reqwest::ReqwestClient;
}
