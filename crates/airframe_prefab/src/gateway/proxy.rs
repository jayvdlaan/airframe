//! Proxy handler and helper functions for the gateway module.

use axum::{
    body::Body,
    http::{header::HeaderValue, HeaderMap, Method, StatusCode, Uri},
    response::Response,
};
use futures_util::TryStreamExt as _;
use http_body_util::BodyExt as _;

use airframe_http::axum_server::{
    get_or_create_gateway_header_policy_registry, get_or_create_gateway_rewriter_registry,
    GatewayRewriter,
};

use super::headers::{filter_hop_by_hop, forward_headers};
use super::RouteTable;

/// Build a target URL by applying registered rewriters, or falling back to the default rewriter.
fn rewrite_target(
    services: &airframe_core::registry::ServiceRegistry,
    up_base: &str,
    tail: &str,
    uri: &Uri,
) -> String {
    let reg = get_or_create_gateway_rewriter_registry(services);
    let rewriters = reg.all();
    if rewriters.is_empty() {
        return default_rewrite(up_base, tail, uri);
    }
    // Apply rewriters in order, passing through
    let mut url = up_base.to_string();
    let mut tail_cur = tail.to_string();
    for r in rewriters.iter() {
        url = r.rewrite(&url, &tail_cur, uri);
        tail_cur = String::new();
    }
    url
}

/// Apply registered header policies on request headers, or fall back to default forward headers.
fn apply_request_header_policies(
    services: &airframe_core::registry::ServiceRegistry,
    headers: &mut HeaderMap,
    uri: &Uri,
    method: &Method,
) {
    let reg = get_or_create_gateway_header_policy_registry(services);
    let policies = reg.all();
    if policies.is_empty() {
        apply_default_forward_headers(headers, uri, method);
        return;
    }
    let mut hdrs = headers.clone();
    for p in policies.iter() {
        p.on_request(&mut hdrs);
    }
    *headers = hdrs;
}

/// Apply registered header policies on response headers (no-op if none registered).
fn apply_response_header_policies(
    services: &airframe_core::registry::ServiceRegistry,
    headers: &mut HeaderMap,
) {
    let reg = get_or_create_gateway_header_policy_registry(services);
    let policies = reg.all();
    for p in policies.iter() {
        p.on_response(headers);
    }
}

/// Zero-copy proxy path via hyper (HTTP-only, GET/HEAD only).
async fn proxy_zero_copy(
    hc: hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        http_body_util::Empty<axum::body::Bytes>,
    >,
    services: &airframe_core::registry::ServiceRegistry,
    parts: &axum::http::request::Parts,
    target: &str,
    method: &Method,
) -> Result<Response, StatusCode> {
    let mut req_builder = hyper::Request::builder().method(method.clone()).uri(target);

    let fwd = forward_headers(&parts.headers);
    let headers = req_builder.headers_mut().unwrap();
    for (name, value) in fwd.iter() {
        headers.insert(name, value.clone());
    }

    // No request body for GET/HEAD in zero-copy path
    let hreq = req_builder
        .body(http_body_util::Empty::new())
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let hresp = hc
        .request(hreq)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status = hresp.status();
    let mut out = Response::builder().status(status);
    let mut headers_out = filter_hop_by_hop(hresp.headers());

    // Insert debug header to make zero-copy path observable in tests.
    // Only present in debug/test builds to avoid leaking in release.
    #[cfg(debug_assertions)]
    headers_out.insert(
        axum::http::header::HeaderName::from_static("x-gw-zero-copy"),
        HeaderValue::from_static("1"),
    );

    apply_response_header_policies(services, &mut headers_out);
    *out.headers_mut().unwrap() = headers_out;

    let body_stream = hresp
        .into_body()
        .into_data_stream()
        .map_err(|_e| std::io::Error::other("resp stream"));
    out.body(Body::from_stream(body_stream))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

/// Send the upstream request via reqwest with either streaming or buffered body.
async fn send_reqwest_request(
    builder: reqwest::RequestBuilder,
    body: Body,
    streaming: bool,
    max_body_bytes: usize,
) -> Result<reqwest::Response, StatusCode> {
    if streaming {
        let stream = futures_util::TryStreamExt::map_err(body.into_data_stream(), |_e| {
            std::io::Error::other("body error")
        });
        let stream = stream.map_ok(|chunk| chunk);
        let rb = reqwest::Body::wrap_stream(stream);
        builder
            .body(rb)
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)
    } else {
        let body_bytes = axum::body::to_bytes(body, max_body_bytes)
            .await
            .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
        builder
            .body(body_bytes.to_vec())
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)
    }
}

/// Build the axum Response from a reqwest response, applying header policies.
async fn build_response_from_reqwest(
    resp: reqwest::Response,
    services: &airframe_core::registry::ServiceRegistry,
    streaming: bool,
) -> Result<Response, StatusCode> {
    let status = resp.status();
    let mut out = Response::builder().status(status);

    let mut headers = filter_hop_by_hop(resp.headers());
    apply_response_header_policies(services, &mut headers);
    *out.headers_mut().unwrap() = headers;

    if streaming {
        let s = futures_util::TryStreamExt::map_err(resp.bytes_stream(), |_e| {
            std::io::Error::other("resp stream")
        });
        out.body(Body::from_stream(s))
            .map_err(|_| StatusCode::BAD_GATEWAY)
    } else {
        let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        out.body(Body::from(bytes))
            .map_err(|_| StatusCode::BAD_GATEWAY)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn proxy_handler(
    req: axum::extract::Request,
    services: airframe_core::registry::ServiceRegistry,
    routes_rx: tokio::sync::watch::Receiver<RouteTable>,
    client: reqwest::Client,
    hclient: Option<
        hyper_util::client::legacy::Client<
            hyper_util::client::legacy::connect::HttpConnector,
            http_body_util::Empty<axum::body::Bytes>,
        >,
    >,
    max_body_bytes: usize,
    streaming: bool,
    zero_copy: bool,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let uri = req.uri().clone();

    // Resolve upstream by longest-prefix match on normalized path only (exclude query)
    let path_only = super::normalize_path(uri.path());
    let table = routes_rx.borrow().clone();
    let (up_base, tail) = table
        .match_upstream(&path_only)
        .ok_or(StatusCode::NOT_FOUND)?;

    // Build target URL using rewriter(s)
    let target = rewrite_target(&services, &up_base, &tail, &uri);

    // Split request into parts and body
    let (mut parts, body) = req.into_parts();

    // Apply header policies on request headers
    apply_request_header_policies(&services, &mut parts.headers, &uri, &method);

    // Zero-copy HTTP proxy via hyper for http:// targets when enabled
    let use_zero_copy = zero_copy
        && target.starts_with("http://")
        && (method == Method::GET || method == Method::HEAD);
    if let (true, Some(hc)) = (use_zero_copy, hclient) {
        return proxy_zero_copy(hc, &services, &parts, &target, &method).await;
    }

    // Fallback to reqwest path (optionally streaming)
    let mut builder = client.request(method_from_axum(&method), &target);
    let fwd = forward_headers(&parts.headers);
    for (name, value) in fwd.iter() {
        builder = builder.header(name, value.clone());
    }

    let resp = send_reqwest_request(builder, body, streaming, max_body_bytes).await?;
    build_response_from_reqwest(resp, &services, streaming).await
}

fn method_from_axum(m: &Method) -> reqwest::Method {
    // Map Axum/HTTP method to reqwest Method
    match *m {
        Method::GET => reqwest::Method::GET,
        Method::POST => reqwest::Method::POST,
        Method::PUT => reqwest::Method::PUT,
        Method::DELETE => reqwest::Method::DELETE,
        Method::HEAD => reqwest::Method::HEAD,
        Method::OPTIONS => reqwest::Method::OPTIONS,
        Method::PATCH => reqwest::Method::PATCH,
        _ => reqwest::Method::GET,
    }
}

pub(super) fn default_rewrite(upstream_base: &str, tail: &str, uri: &Uri) -> String {
    let mut target = upstream_base.to_string();
    if !tail.is_empty() {
        if !target.ends_with('/') && !tail.starts_with('/') {
            target.push('/');
        }
        target.push_str(tail);
    }
    if let Some(q) = uri.query() {
        target.push('?');
        target.push_str(q);
    }
    target
}

pub(super) struct DefaultRewriter;
impl GatewayRewriter for DefaultRewriter {
    fn rewrite(&self, upstream_base: &str, tail: &str, uri: &Uri) -> String {
        default_rewrite(upstream_base, tail, uri)
    }
}

fn apply_default_forward_headers(h: &mut HeaderMap, orig: &Uri, method: &Method) {
    // x-forwarded-host
    if let Some(host) = h
        .get("host")
        .and_then(|v| v.to_str().ok())
        .or_else(|| orig.host())
    {
        let _ = h.insert(
            "x-forwarded-host",
            HeaderValue::from_str(host).unwrap_or(HeaderValue::from_static("")),
        );
    }
    let proto = if orig.scheme_str() == Some("https") {
        "https"
    } else {
        "http"
    };
    let _ = h.insert("x-forwarded-proto", HeaderValue::from_static(proto));
    // x-forwarded-for (append if present) — try common client IP headers
    let mut client_ip: Option<String> = None;
    for name in [
        "x-real-ip",
        "x-client-ip",
        "cf-connecting-ip",
        "x-forwarded-for",
    ] {
        if let Some(v) = h.get(name).and_then(|v| v.to_str().ok()) {
            if !v.is_empty() {
                client_ip = Some(v.split(',').next().unwrap_or(v).trim().to_string());
                break;
            }
        }
    }
    if let Some(ip) = client_ip {
        let new_val = if let Some(cur) = h.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if cur.is_empty() {
                ip
            } else {
                format!("{cur}, {ip}")
            }
        } else {
            ip
        };
        let _ = h.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&new_val).unwrap_or(HeaderValue::from_static("")),
        );
    }
    // Trace context: preserve traceparent/tracestate, generate traceparent if missing
    if h.get("traceparent").is_none() {
        // Generate W3C traceparent: 00-<32hex>-<16hex>-01
        let trace_id = random_hex(16);
        let span_id = random_hex(8);
        let tp = format!("00-{}-{}-01", trace_id, span_id);
        let _ = h.insert(
            "traceparent",
            HeaderValue::from_str(&tp).unwrap_or(HeaderValue::from_static(
                "00-00000000000000000000000000000000-0000000000000000-00",
            )),
        );
    }
    let _m = method; // reserved for future method-based policies
}

fn random_hex(bytes: usize) -> String {
    use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};
    let suite = SoftwareCipherSuite::new();
    let b = suite
        .random_bytes(bytes)
        .unwrap_or_else(|_| vec![0u8; bytes]);
    let mut s = String::with_capacity(bytes * 2);
    for v in b {
        s.push_str(&format!("{:02x}", v));
    }
    s
}
