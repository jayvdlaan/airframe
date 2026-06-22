//! airframe_api: Core API specification traits and types.
//! Minimal, runtime-agnostic building blocks to describe and use HTTP APIs.

pub use bytes;
pub use http;

use bytes::Bytes;
use http::{Method, Request, Uri};
use tracing::{instrument, trace};

/// Description of a single API endpoint.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub operation_id: &'static str,
    pub method: Method,
    pub path_template: &'static str, // e.g., "/v1/items/{id}"
}

/// An API specification capable of resolving endpoints and constructing requests.
pub trait ApiSpec: Send + Sync {
    /// Canonical base URI to be used by clients.
    fn base_uri(&self) -> Uri;

    /// Resolve an endpoint by operation id.
    fn endpoint(&self, operation_id: &str) -> Option<Endpoint>;

    /// Build an HTTP request for the provided operation using params/body.
    ///
    /// Implementations may perform template expansion, query/header mapping,
    /// and authentication. `params` is intentionally a serde_json::Value to
    /// avoid prescribing a specific parameter model at this layer.
    fn build_request(
        &self,
        operation_id: &str,
        params: &serde_json::Value,
        body: Option<Bytes>,
    ) -> anyhow::Result<Request<Bytes>>;
}

/// A simple, code-first API spec you can build programmatically.
#[derive(Clone, Debug)]
pub struct CodeSpec {
    base: Uri,
    endpoints: Vec<Endpoint>,
}

impl CodeSpec {
    pub fn new(base: Uri) -> Self {
        Self {
            base,
            endpoints: Vec::new(),
        }
    }

    pub fn route(
        mut self,
        operation_id: &'static str,
        method: Method,
        path_template: &'static str,
    ) -> Self {
        self.endpoints.push(Endpoint {
            operation_id,
            method,
            path_template,
        });
        self
    }

    fn resolve_endpoint(&self, operation_id: &str) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|e| e.operation_id == operation_id)
    }
}

impl ApiSpec for CodeSpec {
    fn base_uri(&self) -> Uri {
        self.base.clone()
    }

    fn endpoint(&self, operation_id: &str) -> Option<Endpoint> {
        self.resolve_endpoint(operation_id).cloned()
    }

    #[instrument(level = "debug", skip(self, params, body))]
    fn build_request(
        &self,
        operation_id: &str,
        params: &serde_json::Value,
        body: Option<Bytes>,
    ) -> anyhow::Result<Request<Bytes>> {
        use anyhow::{bail, Context};
        trace!(
            target = "airframe_api",
            op = operation_id,
            has_body = body.is_some()
        );
        let ep = self
            .resolve_endpoint(operation_id)
            .with_context(|| format!("unknown operation_id: {operation_id}"))?;

        // 1) Build path via simple `{name}` replacement from params[name] as string.
        let path = ep.path_template.to_string();
        trace!(
            target = "airframe_api",
            op = operation_id,
            path_template_len = path.len(),
            param_keys = params.as_object().map(|o| o.len()).unwrap_or(0)
        );
        // Collect placeholders
        let mut missing: Vec<String> = Vec::new();
        let mut out = String::with_capacity(path.len());
        let mut i = 0;
        while let Some(start) = path[i..].find('{') {
            let start_abs = i + start;
            out.push_str(&path[i..start_abs]);
            let rest = &path[start_abs + 1..];
            if let Some(end_rel) = rest.find('}') {
                let key = &rest[..end_rel];
                let val = params.get(key).and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                });
                match val {
                    Some(v) => out.push_str(&urlencoding::encode(&v)),
                    None => {
                        missing.push(key.to_string()); /* leave empty */
                    }
                }
                i = start_abs + 1 + end_rel + 1;
            } else {
                // unmatched '{' – copy rest and break
                out.push_str(&path[start_abs + 1..]);
                i = path.len();
                break;
            }
        }
        out.push_str(&path[i..]);
        if !missing.is_empty() {
            bail!("missing path params: {}", missing.join(","));
        }

        // 2) Compute URL = base + path, append simple query from params["query"] if object
        let mut url = self.base.to_string();
        if url.ends_with('/') && out.starts_with('/') {
            url.pop();
        }
        url.push_str(&out);
        if let Some(q) = params.get("query").and_then(|v| v.as_object()) {
            let mut first = true;
            for (k, v) in q.iter() {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                url.push(if first { '?' } else { '&' });
                first = false;
                url.push_str(&urlencoding::encode(k));
                url.push('=');
                url.push_str(&urlencoding::encode(&val));
            }
        }

        // 3) Build Request with optional body
        let builder = http::Request::builder().method(ep.method.clone()).uri(url);
        let body_bytes = body.unwrap_or_default();
        let req = builder
            .body(body_bytes)
            .context("failed to build request")?;
        Ok(req)
    }
}

/// Very lightweight validation to ensure a path template has balanced `{` and `}` in order.
/// This does not validate parameter names, it only checks brace pairing.
pub fn is_valid_path_template(s: &str) -> bool {
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '{' => depth += 1,
            // Do not allow nested placeholders like {a{b}}
            // Templates are expected to be flat: zero or one level of braces.
            // If depth exceeds 1 at any time, treat as invalid.
            _ if depth > 1 => return false,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Minimal, portable API error representation suitable for JSON.
/// Intended for cross-crate surface error types.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct ApiError {
    /// Numeric code following HTTP semantics where possible (e.g., 404).
    pub code: u16,
    /// Optional human-friendly message. Can be omitted to save bytes.
    #[serde(default)]
    pub message: Option<String>,
}

impl ApiError {
    /// Helper: true if code is in 400..=499
    pub fn is_client_error(&self) -> bool {
        (400..=499).contains(&self.code)
    }
    /// Helper: true if code is in 500..=599
    pub fn is_server_error(&self) -> bool {
        (500..=599).contains(&self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Path template validation tests
    #[test]
    fn valid_path_templates() {
        assert!(is_valid_path_template("/v1/items"));
        assert!(is_valid_path_template("/v1/items/{id}"));
        assert!(is_valid_path_template("/v1/{resource}/{id}/action"));
        assert!(is_valid_path_template(""));
    }

    #[test]
    fn invalid_path_templates() {
        assert!(!is_valid_path_template("/v1/items/{id")); // unclosed
        assert!(!is_valid_path_template("/v1/items/id}")); // extra close
        assert!(!is_valid_path_template("/v1/{a{b}}")); // nested
    }

    // ApiError tests
    #[test]
    fn api_error_is_client_error() {
        assert!(ApiError {
            code: 400,
            message: None
        }
        .is_client_error());
        assert!(ApiError {
            code: 404,
            message: None
        }
        .is_client_error());
        assert!(ApiError {
            code: 499,
            message: None
        }
        .is_client_error());
        assert!(!ApiError {
            code: 500,
            message: None
        }
        .is_client_error());
        assert!(!ApiError {
            code: 200,
            message: None
        }
        .is_client_error());
    }

    #[test]
    fn api_error_is_server_error() {
        assert!(ApiError {
            code: 500,
            message: None
        }
        .is_server_error());
        assert!(ApiError {
            code: 503,
            message: None
        }
        .is_server_error());
        assert!(ApiError {
            code: 599,
            message: None
        }
        .is_server_error());
        assert!(!ApiError {
            code: 400,
            message: None
        }
        .is_server_error());
        assert!(!ApiError {
            code: 200,
            message: None
        }
        .is_server_error());
    }

    #[test]
    fn api_error_default() {
        let err = ApiError::default();
        assert_eq!(err.code, 0);
        assert_eq!(err.message, None);
    }

    #[test]
    fn api_error_with_message() {
        let err = ApiError {
            code: 404,
            message: Some("Not found".to_string()),
        };
        assert_eq!(err.code, 404);
        assert_eq!(err.message.as_deref(), Some("Not found"));
    }

    // CodeSpec tests
    #[test]
    fn code_spec_new_and_base_uri() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base.clone());
        assert_eq!(spec.base_uri(), base);
    }

    #[test]
    fn code_spec_route_and_endpoint() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base)
            .route("get_item", Method::GET, "/v1/items/{id}")
            .route("create_item", Method::POST, "/v1/items");

        let ep = spec.endpoint("get_item");
        assert!(ep.is_some());
        let ep = ep.unwrap();
        assert_eq!(ep.operation_id, "get_item");
        assert_eq!(ep.method, Method::GET);
        assert_eq!(ep.path_template, "/v1/items/{id}");

        assert!(spec.endpoint("unknown").is_none());
    }

    #[test]
    fn code_spec_build_request_simple() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base).route("list_items", Method::GET, "/v1/items");

        let params = serde_json::json!({});
        let req = spec.build_request("list_items", &params, None).unwrap();

        assert_eq!(req.method(), Method::GET);
        assert_eq!(req.uri().to_string(), "https://api.example.com/v1/items");
    }

    #[test]
    fn code_spec_build_request_with_path_param() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base).route("get_item", Method::GET, "/v1/items/{id}");

        let params = serde_json::json!({"id": "123"});
        let req = spec.build_request("get_item", &params, None).unwrap();

        assert_eq!(
            req.uri().to_string(),
            "https://api.example.com/v1/items/123"
        );
    }

    #[test]
    fn code_spec_build_request_with_query_params() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base).route("list_items", Method::GET, "/v1/items");

        let params = serde_json::json!({
            "query": {
                "page": 1,
                "limit": "10"
            }
        });
        let req = spec.build_request("list_items", &params, None).unwrap();

        let uri = req.uri().to_string();
        assert!(uri.contains("page=1"));
        assert!(uri.contains("limit=10"));
    }

    #[test]
    fn code_spec_build_request_missing_param_fails() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base).route("get_item", Method::GET, "/v1/items/{id}");

        let params = serde_json::json!({});
        let result = spec.build_request("get_item", &params, None);

        assert!(result.is_err());
    }

    #[test]
    fn code_spec_build_request_unknown_operation_fails() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base);

        let params = serde_json::json!({});
        let result = spec.build_request("unknown", &params, None);

        assert!(result.is_err());
    }

    #[test]
    fn code_spec_build_request_with_body() {
        let base: Uri = "https://api.example.com".parse().unwrap();
        let spec = CodeSpec::new(base).route("create_item", Method::POST, "/v1/items");

        let params = serde_json::json!({});
        let body = Bytes::from(r#"{"name":"test"}"#);
        let req = spec
            .build_request("create_item", &params, Some(body.clone()))
            .unwrap();

        assert_eq!(req.method(), Method::POST);
        assert_eq!(req.body(), &body);
    }
}
