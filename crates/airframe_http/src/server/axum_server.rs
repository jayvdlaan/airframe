//! server::axum_server — the minimal Axum server adapter and the bound-address
//! type published to the service registry.
//!
//! The Airframe `Module` integration lives in [`super::module`] and the
//! bind-address resolution logic in [`super::bind`]; both are re-exported here
//! to preserve the historical `axum_server::*` public paths.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;

// Re-export the Module integration so the historical
// `airframe_http::axum_server::AxumServerModule` path keeps resolving.
#[cfg(feature = "module")]
pub use super::module::AxumServerModule;

/// Minimal Axum server adapter with a simple serve loop.
pub struct AxumServer {
    addr: SocketAddr,
    router: Router,
}

impl AxumServer {
    pub fn new(router: Router, addr: SocketAddr) -> Self {
        Self { addr, router }
    }

    /// Bind the address and serve the provided router until the task is cancelled.
    ///
    /// Uses `into_make_service_with_connect_info::<SocketAddr>()` so handlers
    /// that need the client peer address can extract it via the
    /// `axum::extract::ConnectInfo<SocketAddr>` extractor (e.g. for IP
    /// logging on direct connections, when no reverse proxy sets
    /// `X-Forwarded-For`).
    pub async fn serve(self) -> Result<(), std::io::Error> {
        let listener = TcpListener::bind(self.addr).await?;
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .map_err(|e| std::io::Error::other(format!("axum serve error: {e}")))
    }
}

/// Axum HTTP server integrated with the Airframe Module system.
/// Published bound address type for registry exposure.
#[derive(Clone, Debug)]
pub struct BoundAddr(pub SocketAddr);
