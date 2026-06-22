//! Canonical HTTP mapping and constants for health probes.
//! This module provides pure helpers (no HTTP framework types) to convert
//! health status into HTTP status codes and bodies, and constants for
//! readiness/liveness paths.

use crate::HealthStatus;

/// Default readiness probe path.
pub const PATH_READINESS: &str = "/readyz";
/// Default liveness probe path.
pub const PATH_LIVENESS: &str = "/healthz"; // keeping historical name used in this workspace

/// Maps a HealthStatus to an HTTP status code and a plain-text body.
/// Semantics:
/// - Healthy => 200
/// - Degraded(msg) => 200 with body "degraded: {msg}"
/// - Unhealthy(msg) =>
///   - For liveness checks: 500
///   - For readiness checks: 503
///
/// This helper returns the common readiness-style mapping (treats Unhealthy as 503)
/// because that's what most callers want for simple probes. If you specifically
/// need liveness mapping (500), use `map_status_to_http_liveness`.
pub fn map_status_to_http_readiness(status: &HealthStatus) -> (u16, String) {
    match status {
        HealthStatus::Healthy => (200, "ready".into()),
        HealthStatus::Degraded(msg) => (200, format!("degraded: {}", msg)),
        HealthStatus::Unhealthy(msg) => (503, format!("unhealthy: {}", msg)),
    }
}

/// Liveness mapping variant where Unhealthy -> 500.
pub fn map_status_to_http_liveness(status: &HealthStatus) -> (u16, String) {
    match status {
        HealthStatus::Healthy => (200, "healthy".into()),
        HealthStatus::Degraded(msg) => (200, format!("degraded: {}", msg)),
        HealthStatus::Unhealthy(msg) => {
            // Treat transitional states like "starting"/"stopping" as not-yet-live (503)
            // to mirror historical behavior in this workspace, while permanent failures
            // map to 500.
            let m = msg.to_lowercase();
            if m == "starting" || m == "stopping" {
                (503, format!("unhealthy: {}", msg))
            } else {
                (500, format!("unhealthy: {}", msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_mapping() {
        assert_eq!(map_status_to_http_readiness(&HealthStatus::Healthy).0, 200);
        assert_eq!(
            map_status_to_http_readiness(&HealthStatus::Degraded("x".into())).0,
            200
        );
        let (code, body) =
            map_status_to_http_readiness(&HealthStatus::Unhealthy("starting".into()));
        assert_eq!(code, 503);
        assert!(body.contains("starting"));
    }

    #[test]
    fn liveness_mapping() {
        assert_eq!(map_status_to_http_liveness(&HealthStatus::Healthy).0, 200);
        assert_eq!(
            map_status_to_http_liveness(&HealthStatus::Degraded("x".into())).0,
            200
        );
        assert_eq!(
            map_status_to_http_liveness(&HealthStatus::Unhealthy("boom".into())).0,
            500
        );
    }
}
