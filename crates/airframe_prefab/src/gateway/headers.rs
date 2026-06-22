//! Hop-by-hop header filtering utilities for the gateway proxy.

use axum::http::HeaderMap;

/// Returns true if the given header name (lowercase) is a hop-by-hop header
/// that should not be forwarded by a proxy.
pub(super) fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Filter out hop-by-hop headers from the given HeaderMap, returning a new
/// HeaderMap containing only the non-hop-by-hop headers.
pub(super) fn filter_hop_by_hop(headers: &HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

/// Filter hop-by-hop headers and also remove `host` and `content-length` (which
/// the downstream HTTP client will recompute).
pub(super) fn forward_headers(source: &HeaderMap) -> HeaderMap {
    let filtered = filter_hop_by_hop(source);
    let mut out = HeaderMap::new();
    for (name, value) in filtered.iter() {
        if matches!(name.as_str(), "host" | "content-length") {
            continue;
        }
        out.insert(name, value.clone());
    }
    out
}
